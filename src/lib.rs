//! Implementation of an interval tree ([`interval_tree::IntervalTree`]) that works with inclusive/exclusive
//! bounds, as well as unbounded intervals. It is based on the
//! data structure described in Cormen et al.
//! (2009, Section 14.3: Interval trees, pp. 348–354). It provides methods
//! for "stabbing queries" (as in "is point `p` or an interval `i` contained in any intervals
//! in the tree of intervals?"), as well as helpers to get the difference between a queried
//! interval and the database (in order to find subsegments not covered), and the list of
//! intervals in the database overlapping a queried interval.
//!
//! Note that any type satisfying the [`Ord`] trait can be stored in this tree.

/// An interval tree implemented with a binary search tree.
pub mod interval_tree;
mod node;
