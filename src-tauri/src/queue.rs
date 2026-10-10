//! The play queue: which song comes next, in linear or shuffle order.
//!
//! This module knows nothing about audio or the database. It only juggles
//! track ids, which makes it easy to test (see the bottom of the file).
//!
//! How it works: `tracks` is the list the user started playing from, in the
//! order it was shown on screen. `order` is the sequence in which we visit
//! that list, stored as indexes into `tracks`:
//!
//! ```text
//! tracks = [id 10, id 11, id 12, id 13]
//! linear : order = [0, 1, 2, 3]
//! shuffle: order = [2, 0, 3, 1]     (a random arrangement)
//! pos    : where we are in `order`
//! ```
//!
//! "Next" is always `pos + 1` and "previous" is always `pos - 1`. Shuffle
//! only changes what `order` contains. This means that in shuffle mode every
//! song plays exactly once before any song repeats, and "previous" really
//! does return to the song you just heard.
//!
//! The two modes differ at the ends of `order`. A linear queue stops there.
//! A shuffled queue is a ring: "next" on the last song wraps round to the
//! first, and "previous" on the first wraps round to the last, so it never
//! runs out in either direction.
//!
//! Learning links:
//! - `Vec`: <https://doc.rust-lang.org/book/ch08-01-vectors.html>
//! - Methods and `&self` / `&mut self`:
//!   <https://doc.rust-lang.org/book/ch05-03-method-syntax.html>

use rand::seq::SliceRandom;

#[derive(Debug, Default)]
pub struct Queue {
    /// Track ids, in the order the UI showed them.
    tracks: Vec<i64>,
    /// The order we play them in: indexes into `tracks`.
    order: Vec<usize>,
    /// Our place in `order`.
    pos: usize,
    shuffle: bool,
}

impl Queue {
    /// An empty queue.
    pub fn new(shuffle: bool) -> Self {
        Self {
            shuffle,
            ..Self::default()
        }
    }

    /// Replaces the queue with `tracks` and makes `tracks[start_index]` the
    /// current song. Called when the user double-clicks a song.
    pub fn set_tracks(&mut self, tracks: Vec<i64>, start_index: usize) {
        self.tracks = tracks;
        // Guard against an out-of-range index from the UI.
        let start = start_index.min(self.tracks.len().saturating_sub(1));
        self.rebuild_order(start);
    }

    /// The id of the current song, or `None` if the queue is empty.
    pub fn current(&self) -> Option<i64> {
        let index = *self.order.get(self.pos)?;
        Some(self.tracks[index])
    }

    /// Moves to the next song and returns its id.
    ///
    /// On the last song, a shuffled queue wraps round to its first song. A
    /// linear queue returns `None` without moving.
    pub fn advance(&mut self) -> Option<i64> {
        if self.pos + 1 < self.order.len() {
            self.pos += 1;
        } else if self.wraps() {
            self.pos = 0;
        } else {
            return None;
        }
        self.current()
    }

    /// Moves to the previous song and returns its id.
    ///
    /// On the first song, a shuffled queue wraps round to its last song:
    /// one that has not been played yet this time round, since the song
    /// shuffle was started from is always first. A linear queue returns
    /// `None` without moving.
    pub fn go_back(&mut self) -> Option<i64> {
        if self.pos > 0 {
            self.pos -= 1;
        } else if self.wraps() {
            self.pos = self.order.len() - 1;
        } else {
            return None;
        }
        self.current()
    }

    /// Whether stepping past either end of `order` carries on from the
    /// other end. True for a shuffled queue, unless it has no other song
    /// to wrap round to.
    fn wraps(&self) -> bool {
        self.shuffle && self.order.len() > 1
    }

    /// Turns shuffle on or off without interrupting the current song.
    ///
    /// Turning it on shuffles everything *else* behind the current song;
    /// turning it off continues in list order from the current song.
    pub fn set_shuffle(&mut self, shuffle: bool) {
        if self.shuffle == shuffle {
            return;
        }
        self.shuffle = shuffle;
        if let Some(&current_index) = self.order.get(self.pos) {
            self.rebuild_order(current_index);
        }
    }

    pub fn shuffle(&self) -> bool {
        self.shuffle
    }

    /// Every track id in the queue, in the order the UI showed them (not
    /// the shuffled order). Returned as a borrowed slice, `&[i64]`: a
    /// read-only view of the `Vec` that copies nothing.
    /// <https://doc.rust-lang.org/book/ch04-03-slices.html>
    pub fn tracks(&self) -> &[i64] {
        &self.tracks
    }

    /// Number of songs in the queue.
    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    /// How far through the queue we are (0-based).
    pub fn position(&self) -> usize {
        self.pos
    }

    /// Recomputes `order` and `pos` so that `tracks[current_index]` is the
    /// current song.
    fn rebuild_order(&mut self, current_index: usize) {
        // `(0..n).collect()` builds [0, 1, 2, ..., n-1].
        self.order = (0..self.tracks.len()).collect();
        if self.order.is_empty() {
            self.pos = 0;
            return;
        }

        if self.shuffle {
            // Put the current song first, then shuffle the rest behind it.
            self.order.swap(0, current_index);
            self.order[1..].shuffle(&mut rand::rng());
            self.pos = 0;
        } else {
            // In linear mode the order is just the list itself.
            self.pos = current_index;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_queue_has_nothing_to_play() {
        let mut queue = Queue::new(false);
        assert_eq!(queue.current(), None);
        assert_eq!(queue.advance(), None);
        assert_eq!(queue.go_back(), None);
    }

    #[test]
    fn linear_plays_in_list_order_and_stops_at_the_end() {
        let mut queue = Queue::new(false);
        queue.set_tracks(vec![10, 11, 12], 1);

        assert_eq!(queue.current(), Some(11));
        assert_eq!(queue.advance(), Some(12));
        assert_eq!(queue.advance(), None); // end of the list
        assert_eq!(queue.current(), Some(12)); // ...and we did not move
        assert_eq!(queue.go_back(), Some(11));
        assert_eq!(queue.go_back(), Some(10));
        assert_eq!(queue.go_back(), None);
    }

    #[test]
    fn shuffle_starts_on_the_chosen_song_and_plays_each_song_once() {
        let mut queue = Queue::new(true);
        let ids: Vec<i64> = (1..=50).collect();
        queue.set_tracks(ids.clone(), 7);

        assert_eq!(queue.current(), Some(8)); // ids[7]

        // A shuffled queue never ends, so take one lap of it: the first
        // song plus the 49 after it.
        let mut played = vec![queue.current().unwrap()];
        for _ in 1..ids.len() {
            played.push(queue.advance().unwrap());
        }
        // With 50 songs the chance of a shuffle producing list order is nil.
        assert_ne!(played, ids[7..].to_vec());
        played.sort();
        assert_eq!(played, ids);
    }

    #[test]
    fn previous_in_shuffle_returns_to_the_song_just_played() {
        let mut queue = Queue::new(true);
        queue.set_tracks((1..=20).collect(), 0);
        let first = queue.current();
        let second = queue.advance();
        assert_eq!(queue.go_back(), first);
        assert_eq!(queue.advance(), second);
    }

    #[test]
    fn shuffle_wraps_round_at_both_ends() {
        let mut queue = Queue::new(true);
        queue.set_tracks((1..=5).collect(), 2);
        let first = queue.current();

        // "Previous" on the first song goes to the last song of the lap...
        let last = queue.go_back();
        assert!(last.is_some());
        assert_ne!(last, first);
        assert_eq!(queue.position(), 4);
        // ...and "next" from there comes back round to the first.
        assert_eq!(queue.advance(), first);
        assert_eq!(queue.position(), 0);

        // A second lap plays the songs in the same order as the first.
        let lap: Vec<_> = (0..5).map(|_| queue.advance()).collect();
        assert_eq!(lap.last(), Some(&first));
        assert_eq!(lap[3], last);
    }

    #[test]
    fn shuffle_with_one_song_has_nowhere_to_wrap_to() {
        let mut queue = Queue::new(true);
        queue.set_tracks(vec![10], 0);
        assert_eq!(queue.advance(), None);
        assert_eq!(queue.go_back(), None);
        assert_eq!(queue.current(), Some(10));
    }

    #[test]
    fn toggling_shuffle_keeps_the_current_song() {
        let mut queue = Queue::new(false);
        queue.set_tracks(vec![10, 11, 12, 13], 2);

        queue.set_shuffle(true);
        assert_eq!(queue.current(), Some(12));

        queue.set_shuffle(false);
        assert_eq!(queue.current(), Some(12));
        assert_eq!(queue.advance(), Some(13)); // back to list order
    }

    #[test]
    fn out_of_range_start_index_is_clamped() {
        let mut queue = Queue::new(false);
        queue.set_tracks(vec![10, 11], 99);
        assert_eq!(queue.current(), Some(11));
    }
}
