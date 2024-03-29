use crate::node::{Node, Range};

use std::borrow::Borrow;
use std::cmp::Ordering;
use std::cmp::Ordering::*;
use std::fmt;
use std::mem;
use std::ops::Bound;
use std::ops::Bound::*;
use std::ops::RangeBounds;
#[cfg(any(feature="serde", test))]
use serde::{Serialize, Deserialize};

/// The interval tree storing all the underlying intervals.
///
/// There are three ways to create an interval tree.
/// ```
/// use unbounded_interval_tree::interval_tree::IntervalTree;
///
/// // 1. Create an empty default interval tree.
/// let mut interval_tree = IntervalTree::default();
/// assert!(interval_tree.is_empty());
/// interval_tree.insert(0..9);
/// interval_tree.insert(27..);
/// assert_eq!(interval_tree.len(), 2);
///
/// // 2. Create an interval tree from an iterator.
/// let ranges = vec!["hello"..="hi", "Allo"..="Bonjour"];
/// let interval_tree = ranges.into_iter().collect::<IntervalTree<_>>();
/// assert_eq!(interval_tree.len(), 2);
///
/// // 3. Create an interval tree from an array.
/// let ranges = [(1, 5)..(1,9), (2, 3)..(3, 7)];
/// let interval_tree = IntervalTree::from(ranges);
/// assert_eq!(interval_tree.len(), 2);
/// ```
#[cfg_attr(any(feature="serde", test), derive(Serialize, Deserialize))]
#[derive(Clone, Debug, PartialEq)]
pub struct IntervalTree<K> {
    root: Option<Box<Node<K>>>,
    size: usize,
}

impl<K> fmt::Display for IntervalTree<K>
where
    K: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self.root {
            Some(ref root) => write!(f, "{}", root),
            None => write!(f, "Empty tree"),
        }
    }
}

impl<K> Default for IntervalTree<K> {
    fn default() -> IntervalTree<K> {
        IntervalTree {
            root: None,
            size: 0,
        }
    }
}

/// Creates an [`IntervalTree`] from an iterator of elements
/// satisfying the [`RangeBounds`] trait.
impl<K, R> FromIterator<R> for IntervalTree<K>
where
    K: Ord + Clone,
    R: RangeBounds<K>,
{
    fn from_iter<T: IntoIterator<Item = R>>(iter: T) -> Self {
        let mut interval_tree = Self::default();

        for interval in iter {
            interval_tree.insert(interval);
        }

        interval_tree
    }
}

impl<K, R, const N: usize> From<[R; N]> for IntervalTree<K>
where
    K: Ord + Clone,
    R: RangeBounds<K>,
{
    fn from(intervals: [R; N]) -> Self {
        let mut interval_tree = Self::default();

        for interval in intervals {
            interval_tree.insert(interval);
        }

        interval_tree
    }
}

impl<K> IntervalTree<K> {
    /// Produces an inorder iterator for the interval tree.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::ops::Bound::Included;
    /// use unbounded_interval_tree::interval_tree::IntervalTree;
    ///
    /// let mut tree = IntervalTree::default();
    ///
    /// tree.insert((Included(0), Included(10)));
    /// tree.insert((Included(-5), Included(-1)));
    /// tree.insert((Included(20), Included(30)));
    ///
    /// let mut iter = tree.iter();
    /// assert_eq!(iter.next(), Some(&(Included(-5), Included(-1))));
    /// assert_eq!(iter.next(), Some(&(Included(0), Included(10))));
    /// assert_eq!(iter.next(), Some(&(Included(20), Included(30))));
    /// assert_eq!(iter.next(), None);
    /// ```
    pub fn iter<'a>(&'a self) -> IntervalTreeIter<'a, K> {
        IntervalTreeIter {
            to_visit: vec![],
            curr: &self.root,
        }
    }

    /// Inserts an interval `range` into the interval tree. Insertions respect the
    /// binary search properties of this tree.
    /// It is ok to insert a `range` that overlaps with an existing interval in the tree.
    ///
    /// An improvement to come is to rebalance the tree (following an AVL or a red-black scheme).
    ///
    /// # Examples
    ///
    /// ```
    /// use std::ops::Bound::{Included, Excluded, Unbounded};
    /// use unbounded_interval_tree::interval_tree::IntervalTree;
    ///
    /// let mut int_tree = IntervalTree::default();
    ///
    /// int_tree.insert((Included(5), Excluded(9)));
    /// int_tree.insert(..=10);
    ///
    /// let mut str_tree: IntervalTree<&str> = IntervalTree::default();
    ///
    /// str_tree.insert("Noria"..);
    /// ```
    pub fn insert<R>(&mut self, range: R)
    where
        K: Ord + Clone,
        R: RangeBounds<K>,
    {
        let range = (range.start_bound().cloned(), range.end_bound().cloned());
        self.size += 1;

        // If the tree is empty, put new node at the root.
        if self.root.is_none() {
            self.root = Some(Box::new(Node::new(range)));
            return;
        }

        // Otherwise, walk down the tree and insert when we reach leaves.
        // TODO(jonathangb): Rotate tree?
        let mut curr = self.root.as_mut().unwrap();
        loop {
            curr.maybe_update_value(&range.1);

            match Self::cmp(&curr.key, &range) {
                Equal => return, // Don't insert a redundant key.
                Less => {
                    match curr.right {
                        None => {
                            curr.right = Some(Box::new(Node::new(range)));
                            return;
                        }
                        Some(ref mut node) => curr = node,
                    };
                }
                Greater => {
                    match curr.left {
                        None => {
                            curr.left = Some(Box::new(Node::new(range)));
                            return;
                        }
                        Some(ref mut node) => curr = node,
                    };
                }
            };
        }
    }

    /// A "stabbing query" in the jargon: returns whether or not a point `p`
    /// is contained in any of the intervals stored in the tree.
    ///
    /// The given point may be of a borrowed form of the stored type `K`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::ops::Bound::{Excluded, Unbounded};
    /// use unbounded_interval_tree::interval_tree::IntervalTree;
    ///
    /// let mut int_tree = IntervalTree::default();
    ///
    /// int_tree.insert((Excluded(5), Unbounded));
    ///
    /// assert!(int_tree.contains_point(&100));
    /// assert!(!int_tree.contains_point(&5));
    /// ```
    ///
    /// Note that we can work with any type that implements the [`Ord`] trait, so
    /// we are not limited to just integers.
    ///
    /// ```
    /// use std::ops::Bound::{Excluded, Unbounded};
    /// use unbounded_interval_tree::interval_tree::IntervalTree;
    ///
    /// let mut str_tree = IntervalTree::default();
    ///
    /// str_tree.insert((Excluded(String::from("Noria")), Unbounded));
    ///
    /// // Borrowed form (`str`) of `String`.
    /// assert!(!str_tree.contains_point("Noria"));
    /// // Also works with non-borrowed form.
    /// assert!(str_tree.contains_point(&String::from("Zebra")));
    /// ```
    pub fn contains_point<Q>(&self, p: &Q) -> bool
    where
        K: Ord + Borrow<Q>,
        Q: Ord + ?Sized,
    {
        self.contains_interval(&(Included(p), Included(p)))
    }

    /// An alternative "stabbing query": returns whether or not an interval `range`
    /// is fully covered by the intervals stored in the tree.
    ///
    /// The given `range` may have bounds that are of a borrowed form of the stored type `K`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::ops::Bound::{Included, Excluded, Unbounded};
    /// use unbounded_interval_tree::interval_tree::IntervalTree;
    ///
    /// let mut tree = IntervalTree::default();
    ///
    /// tree.insert((Included(20), Included(30)));
    /// tree.insert((Excluded(30), Excluded(50)));
    ///
    /// assert!(tree.contains_interval(&(20..=40)));
    /// // Borrowed form of the key works as well.
    /// assert!(!tree.contains_interval(&(&30..=&50)));
    /// ```
    ///
    /// Again, the given `range` can be any type implementing [`Ord`].
    ///
    /// ```
    /// use std::ops::Bound::{Included, Excluded, Unbounded};
    /// use unbounded_interval_tree::interval_tree::IntervalTree;
    ///
    /// let mut tree: IntervalTree<&str> = IntervalTree::default();
    ///
    /// let key1 = (Included("a"), Excluded("h"));
    /// let key2 = (Excluded("M"), Excluded("O"));
    ///
    /// tree.insert(key1.clone());
    /// tree.insert(key2);
    ///
    /// assert!(tree.contains_interval(&("a".."h")));
    /// assert!(!tree.contains_interval(&("N"..="O")));
    /// // Sometimes, we have to disambiguate the key type.
    /// assert!(tree.contains_interval::<&str, _>(&key1));
    /// ```
    pub fn contains_interval<Q, R>(&self, range: &R) -> bool
    where
        K: Ord + Borrow<Q>,
        R: RangeBounds<Q>,
        Q: Ord + ?Sized,
    {
        self.get_interval_difference(range).is_empty()
    }

    /// Returns the inorder list of all references to intervals stored in the tree that overlaps
    /// with the given `range` (partially or completely).
    ///
    /// The given `range` may have bounds that are of a borrowed form of the stored type `K`.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::ops::Bound::{Included, Excluded, Unbounded};
    /// use unbounded_interval_tree::interval_tree::IntervalTree;
    ///
    /// let mut tree = IntervalTree::default();
    ///
    /// tree.insert((Included(0), Included(5)));
    /// tree.insert((Included(7), Excluded(10)));
    ///
    /// assert_eq!(tree.get_interval_overlaps(&(-5..7)),
    ///            vec![&(Included(0), Included(5))]);
    /// // Borrowed form of the key works as well.
    /// assert!(tree.get_interval_overlaps(&(&10..)).is_empty());
    /// ```
    pub fn get_interval_overlaps<Q, R>(&self, range: &R) -> Vec<&Range<K>>
    where
        K: Ord + Borrow<Q>,
        R: RangeBounds<Q>,
        Q: Ord + ?Sized,
    {
        let curr = &self.root;
        let mut acc = Vec::new();

        Self::get_interval_overlaps_rec(curr, range, &mut acc);
        acc
    }

    /// Returns the ordered list of subintervals in `range` that are not covered by the tree.
    /// This is useful to compute what subsegments of `range` that are not covered by the intervals
    /// stored in the tree.
    ///
    /// If `range` is not covered at all, this simply returns a one element vector
    /// containing the bounds of `range`.
    ///
    /// The given `range` may have bounds that are of a borrowed form of the stored type `K`.
    /// Because all the bounds returned are either from the interval tree of from the `range`, we return
    /// references to these bounds rather than clone them.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::ops::Bound::{Included, Excluded, Unbounded};
    /// use unbounded_interval_tree::interval_tree::IntervalTree;
    ///
    /// let mut tree = IntervalTree::default();
    ///
    /// tree.insert((Included(0), Excluded(10)));
    /// tree.insert((Excluded(10), Included(30)));
    /// tree.insert((Excluded(50), Unbounded));
    ///
    /// assert_eq!(tree.get_interval_difference(&(-5..=30)),
    ///            vec![(Included(&-5), Excluded(&0)),
    ///                 (Included(&10), Included(&10))]);
    /// assert_eq!(tree.get_interval_difference(&(..10)),
    ///            vec![(Unbounded, Excluded(&0))]);
    /// assert!(tree.get_interval_difference(&(100..)).is_empty());
    /// ```
    pub fn get_interval_difference<'a, Q, R>(&'a self, range: &'a R) -> Vec<Range<&'a Q>>
    where
        K: Ord + Borrow<Q>,
        R: RangeBounds<Q>,
        Q: Ord + ?Sized,
    {
        let overlaps = self.get_interval_overlaps(range);

        // If there is no overlap, then the difference is the query `q` itself.
        if overlaps.is_empty() {
            let min = match range.start_bound() {
                Included(x) => Included(x),
                Excluded(x) => Excluded(x),
                Unbounded => Unbounded,
            };
            let max = match range.end_bound() {
                Included(x) => Included(x),
                Excluded(x) => Excluded(x),
                Unbounded => Unbounded,
            };
            return vec![(min, max)];
        }

        let mut acc = Vec::new();
        let first = overlaps.first().unwrap();

        // If q.min < first.min, we have a difference to append.
        match (range.start_bound(), first.start_bound()) {
            (Unbounded, Included(first_min)) => acc.push((Unbounded, Excluded(first_min.borrow()))),
            (Unbounded, Excluded(first_min)) => acc.push((Unbounded, Included(first_min.borrow()))),
            (Included(q_min), Included(first_min)) if q_min < first_min.borrow() => {
                acc.push((Included(q_min), Excluded(first_min.borrow())))
            }
            (Excluded(q_min), Included(first_min)) if q_min < first_min.borrow() => {
                acc.push((Excluded(q_min), Excluded(first_min.borrow())))
            }
            (Excluded(q_min), Excluded(first_min)) if q_min < first_min.borrow() => {
                acc.push((Excluded(q_min), Included(first_min.borrow())))
            }
            (Included(q_min), Excluded(first_min)) if q_min <= first_min.borrow() => {
                acc.push((Included(q_min), Included(first_min.borrow())))
            }
            _ => {}
        };

        // If the max is unbounded, there can't be any difference going forward.
        if first.1 == Unbounded {
            return acc;
        }

        let mut contiguous = &first.1; // keeps track of the maximum of a contiguous interval.
        for overlap in overlaps.iter().skip(1) {
            // If contiguous < overlap.min:
            //   1. We have a difference between contiguous -> overlap.min to fill.
            //     1.1: Note: the endpoints of the difference appended are the opposite,
            //          that is if contiguous was Included, then the difference must
            //          be Excluded, and vice versa.
            //   2. We need to update contiguous to be the new contiguous max.
            // Note: an Included+Excluded at the same point still is contiguous!
            match (&contiguous, &overlap.0) {
                (Included(contiguous_max), Included(overlap_min))
                    if contiguous_max < overlap_min =>
                {
                    acc.push((
                        Excluded(contiguous_max.borrow()),
                        Excluded(overlap_min.borrow()),
                    ));
                    contiguous = &overlap.1;
                }
                (Included(contiguous_max), Excluded(overlap_min))
                    if contiguous_max < overlap_min =>
                {
                    acc.push((
                        Excluded(contiguous_max.borrow()),
                        Included(overlap_min.borrow()),
                    ));
                    contiguous = &overlap.1;
                }
                (Excluded(contiguous_max), Included(overlap_min))
                    if contiguous_max < overlap_min =>
                {
                    acc.push((
                        Included(contiguous_max.borrow()),
                        Excluded(overlap_min.borrow()),
                    ));
                    contiguous = &overlap.1;
                }
                (Excluded(contiguous_max), Excluded(overlap_min))
                    if contiguous_max <= overlap_min =>
                {
                    acc.push((
                        Included(contiguous_max.borrow()),
                        Included(overlap_min.borrow()),
                    ));
                    contiguous = &overlap.1;
                }
                _ => {}
            }

            // If contiguous.max < overlap.max, we set contiguous to the new max.
            match (&contiguous, &overlap.1) {
                (_, Unbounded) => return acc,
                (Included(contiguous_max), Included(overlap_max))
                | (Excluded(contiguous_max), Excluded(overlap_max))
                | (Included(contiguous_max), Excluded(overlap_max))
                    if contiguous_max < overlap_max =>
                {
                    contiguous = &overlap.1
                }
                (Excluded(contiguous_max), Included(overlap_max))
                    if contiguous_max <= overlap_max =>
                {
                    contiguous = &overlap.1
                }
                _ => {}
            };
        }

        // If contiguous.max < q.max, we have a difference to append.
        match (&contiguous, range.end_bound()) {
            (Included(contiguous_max), Included(q_max)) if contiguous_max.borrow() < q_max => {
                acc.push((Excluded(contiguous_max.borrow()), Included(q_max)))
            }
            (Included(contiguous_max), Excluded(q_max)) if contiguous_max.borrow() < q_max => {
                acc.push((Excluded(contiguous_max.borrow()), Excluded(q_max)))
            }
            (Excluded(contiguous_max), Excluded(q_max)) if contiguous_max.borrow() < q_max => {
                acc.push((Included(contiguous_max.borrow()), Excluded(q_max)))
            }
            (Excluded(contiguous_max), Included(q_max)) if contiguous_max.borrow() <= q_max => {
                acc.push((Included(contiguous_max.borrow()), Included(q_max)))
            }
            _ => {}
        };

        acc
    }

    fn get_interval_overlaps_rec<'a, Q, R>(
        curr: &'a Option<Box<Node<K>>>,
        range: &R,
        acc: &mut Vec<&'a Range<K>>,
    ) where
        K: Ord + Borrow<Q>,
        R: RangeBounds<Q>,
        Q: Ord + ?Sized,
    {
        // If we reach None, stop recursing along this subtree.
        let node = match curr {
            None => return,
            Some(node) => node,
        };

        // See if subtree.max < q.min. If that is the case, there is no point
        // in visiting the rest of the subtree (we know that the rest of the intervals
        // will necessarily be smaller than `q`).
        // ~ Recall the ordering rules (as defined in `fn cmp` below). ~
        // -> If subtree.max is Unbounded, subtree.max < q.min is impossible.
        // -> If q.min is Unbounded, subtree.max < q.min is impossible.
        // -> If they are equal, we have 4 cases:
        //  * subtree.max: Included(x) / q.min: Included(x) -> =, we keep visiting the subtree
        //  * subtree.max: Included(x) / q.min: Excluded(x) -> <, condition satisfied
        //  * subtree.max: Excluded(x) / q.min: Included(x) -> <, condition satisfied
        //  * subtree.max: Excluded(x) / q.min: Excluded(x) -> <, condition satisfied
        let max_subtree = match &node.value {
            Included(x) => Some((x.borrow(), 2)),
            Excluded(x) => Some((x.borrow(), 1)),
            Unbounded => None,
        };
        let min_q = match range.start_bound() {
            Included(x) => Some((x, 2)),
            Excluded(x) => Some((x, 3)),
            Unbounded => None,
        };
        match (max_subtree, min_q) {
            (Some(max_subtree), Some(min_q)) if max_subtree < min_q => return,
            _ => {}
        };

        // Search left subtree.
        Self::get_interval_overlaps_rec(&node.left, range, acc);

        // Visit this node.
        // If node.min <= q.max AND node.max >= q.min, we have an intersection.
        // Let's start with the first inequality, node.min <= q.max.
        // -> If node.min is Unbounded, node.min <= q.max is a tautology.
        // -> If q.max is Unbounded, node.min <= q.max is a tautology.
        // -> If they are equal, we have 4 cases:
        //  * node.min: Included(x) / q.max: Included(x) -> =, we go to 2nd inequality
        //  * node.min: Included(x) / q.max: Excluded(x) -> >, 1st inequality not satisfied
        //  * node.min: Excluded(x) / q.max: Included(x) -> >, 1st inequality not satisfied
        //  * node.min: Excluded(x) / q.max: Excluded(x) -> >, 1st inequality not satisfied
        //
        // Notice that after we visit the node, we should visit the right subtree. However,
        // if node.min > q.max, we can skip right visiting the right subtree.
        // -> If node.min is Unbounded, node.min > q.max is impossible.
        // -> If q.max is Unbounded, node.min > q.max is impossible.
        //
        // It just so happens that we already do this check in the match to satisfy
        // the previous first condition. Hence, we decided to add an early return
        // in there, rather than repeat the logic afterwards.
        let min_node = match &node.key.0 {
            Included(x) => Some((x.borrow(), 2)),
            Excluded(x) => Some((x.borrow(), 3)),
            Unbounded => None,
        };
        let max_q = match range.end_bound() {
            Included(x) => Some((x, 2)),
            Excluded(x) => Some((x, 1)),
            Unbounded => None,
        };
        match (min_node, max_q) {
            // If the following condition is met, we do not have an intersection.
            // On top of that, we know that we can skip visiting the right subtree,
            // so we can return eagerly.
            (Some(min_node), Some(max_q)) if min_node > max_q => return,
            _ => {
                // Now we are at the second inequality, node.max >= q.min.
                // -> If node.max is Unbounded, node.max >= q.min is a tautology.
                // -> If q.min is Unbounded, node.max >= q.min is a tautology.
                // -> If they are equal, we have 4 cases:
                //  * node.max: Included(x) / q.min: Included(x) -> =, 2nd inequality satisfied
                //  * node.max: Included(x) / q.min: Excluded(x) -> <, 2nd inequality not satisfied
                //  * node.max: Excluded(x) / q.min: Included(x) -> <, 2nd inequality not satisfied
                //  * node.max: Excluded(x) / q.min: Excluded(x) -> <, 2nd inequality not satisfied
                let max_node = match &node.key.1 {
                    Included(x) => Some((x.borrow(), 2)),
                    Excluded(x) => Some((x.borrow(), 1)),
                    Unbounded => None,
                };

                match (max_node, min_q) {
                    (Some(max_node), Some(min_q)) if max_node < min_q => {}
                    _ => acc.push(&node.key),
                };
            }
        };

        // Search right subtree.
        Self::get_interval_overlaps_rec(&node.right, range, acc);
    }

    /// Removes a random leaf from the tree,
    /// and returns the range stored in the said node.
    ///
    /// The returned value will be `None` if the tree is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::ops::Bound::{Included, Excluded, Unbounded};
    /// use unbounded_interval_tree::interval_tree::IntervalTree;
    ///
    /// let mut tree = IntervalTree::default();
    ///
    /// tree.insert((Included(5), Excluded(9)));
    /// tree.insert((Unbounded, Included(10)));
    ///
    /// assert!(tree.contains_point(&10));
    /// assert!(tree.contains_point(&6));
    ///
    /// let deleted = tree.remove_random_leaf();
    /// assert!(deleted.is_some());
    /// assert!(!tree.contains_point(&10));
    /// assert!(tree.contains_point(&6));
    ///
    /// let deleted = tree.remove_random_leaf();
    /// assert!(deleted.is_some());
    /// assert!(!tree.contains_point(&6));
    ///
    /// let deleted = tree.remove_random_leaf();
    /// assert!(deleted.is_none());
    /// ```
    pub fn remove_random_leaf(&mut self) -> Option<Range<K>>
    where
        K: Ord + Clone,
    {
        use rand::random;

        // If interval tree is empty, just return None.
        if self.root.is_none() {
            return None;
        }

        self.size -= 1;

        let mut curr = self.root.as_mut().unwrap();

        // If we only have one node, delete it right away.
        if curr.left.is_none() && curr.right.is_none() {
            let root = mem::take(&mut self.root).unwrap();
            return Some(root.key);
        }

        // Keep track of visited nodes, because we will need to walk up
        // the tree after deleting the leaf in order to possibly update
        // their value stored.
        // The first element of the tuple is a &mut to the value of the node,
        // whilst the second element is the new potential value to store, based
        // on the non-visited path (recall that this is a BST). It
        // is very much possible that both elements are equal: that would imply that the
        // current value depends solely on the non-visited path, hence the deleted
        // node will have no impact up the tree, at least from the current point.
        let mut path: Vec<(_, _)> = Vec::new();

        // Used to keep track of the direction taken from a node.
        enum Direction {
            LEFT,
            RIGHT,
        }

        // Traverse the tree until we find a leaf.
        let (deleted, new_max) = loop {
            // Note that at this point in the loop, `curr` can't be a leaf.
            // Indeed, we traverse the tree such that `curr` is always an
            // internal node, so that it is easy to replace a leaf from `curr`.
            let direction = if curr.left.is_none() {
                Direction::RIGHT
            } else if curr.right.is_none() {
                Direction::LEFT
            } else if random() {
                Direction::LEFT
            } else {
                Direction::RIGHT
            };
            // End-bound of the current node.
            let curr_end = &curr.key.1;

            // LEFT and RIGHT paths are somewhat repetitive, but this way
            // was the only way to satisfy the borrowchecker...
            match direction {
                Direction::LEFT => {
                    // If we go left and the right path is `None`,
                    // then the right path has no impact towards
                    // the value stored by the current node.
                    // Otherwise, the current node's value might change
                    // to the other branch's max value once we remove the
                    // leaf, so let's keep track of that.
                    let max_other = if curr.right.is_none() {
                        curr_end
                    } else {
                        let other_value = &curr.right.as_ref().unwrap().value;
                        match Self::cmp_endbound(curr_end, other_value) {
                            Greater | Equal => curr_end,
                            Less => other_value,
                        }
                    };

                    // Check if the next node is a leaf. If it is, then we want to
                    // stop traversing, and remove the leaf.
                    let next = curr.left.as_ref().unwrap();
                    if next.is_leaf() {
                        curr.value = max_other.clone();
                        break (mem::take(&mut curr.left).unwrap(), max_other);
                    }

                    // If the next node is *not* a leaf, then we can update the visited path
                    // with the current values, and move on to the next node.
                    path.push((&mut curr.value, max_other));
                    curr = curr.left.as_mut().unwrap();
                }
                Direction::RIGHT => {
                    let max_other = if curr.left.is_none() {
                        curr_end
                    } else {
                        let other_value = &curr.left.as_ref().unwrap().value;
                        match Self::cmp_endbound(curr_end, other_value) {
                            Greater | Equal => curr_end,
                            Less => other_value,
                        }
                    };

                    let next = curr.right.as_ref().unwrap();
                    if next.is_leaf() {
                        curr.value = max_other.clone();
                        break (mem::take(&mut curr.right).unwrap(), max_other);
                    }

                    path.push((&mut curr.value, max_other));
                    curr = curr.right.as_mut().unwrap();
                }
            };
        };

        // We have removed the leaf. Now, we bubble-up the visited path.
        // If the removed node's value impacted its ancestors, then we update
        // the ancestors' value so that they store the new max value in their
        // respective subtree.
        while let Some((value, max_other)) = path.pop() {
            if Self::cmp_endbound(value, max_other) == Equal {
                break;
            }

            match Self::cmp_endbound(value, new_max) {
                Equal => break,
                Greater => *value = new_max.clone(),
                Less => unreachable!("Can't have a new max that is bigger"),
            };
        }

        Some(deleted.key.clone())
    }

    /// Returns the number of ranges stored in the interval tree.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::ops::Bound::{Included, Excluded, Unbounded};
    /// use unbounded_interval_tree::interval_tree::IntervalTree;
    ///
    /// let mut tree = IntervalTree::default();
    ///
    /// assert_eq!(tree.len(), 0);
    ///
    /// tree.insert((Included(5), Excluded(9)));
    /// tree.insert((Unbounded, Included(10)));
    ///
    /// assert_eq!(tree.len(), 2);
    /// ```
    pub fn len(&self) -> usize {
        self.size
    }

    /// Returns `true` if the map contains no element.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::ops::Bound::{Included, Excluded, Unbounded};
    /// use unbounded_interval_tree::interval_tree::IntervalTree;
    ///
    /// let mut tree = IntervalTree::default();
    ///
    /// assert!(tree.is_empty());
    ///
    /// tree.insert((Included(5), Excluded(9)));
    ///
    /// assert!(!tree.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Clear the interval tree, removing all values stored.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::ops::Bound::{Included, Excluded, Unbounded};
    /// use unbounded_interval_tree::interval_tree::IntervalTree;
    ///
    /// let mut tree = IntervalTree::default();
    ///
    /// tree.insert((Included(5), Unbounded));
    /// tree.clear();
    ///
    /// assert!(tree.is_empty());
    /// ```
    pub fn clear(&mut self) {
        self.root = None;
        self.size = 0;
    }

    fn cmp(r1: &Range<K>, r2: &Range<K>) -> Ordering
    where
        K: Ord,
    {
        // Sorting by lower bound, then by upper bound.
        //   -> Unbounded is the smallest lower bound.
        //   -> Unbounded is the biggest upper bound.
        //   -> Included(x) < Excluded(x) for a lower bound.
        //   -> Included(x) > Excluded(x) for an upper bound.

        // Unpacking from a Bound is annoying, so let's map it to an Option<K>.
        // Let's use this transformation to encode the Included/Excluded rules at the same time.
        // Note that topological order is used during comparison, so if r1 and r2 have the same `x`,
        // only then will the 2nd element of the tuple serve as a tie-breaker.
        let r1_min = match &r1.0 {
            Included(x) => Some((x, 1)),
            Excluded(x) => Some((x, 2)),
            Unbounded => None,
        };
        let r2_min = match &r2.0 {
            Included(x) => Some((x, 1)),
            Excluded(x) => Some((x, 2)),
            Unbounded => None,
        };

        match (r1_min, r2_min) {
            (None, None) => {} // Left-bounds are equal, we can't return yet.
            (None, Some(_)) => return Less,
            (Some(_), None) => return Greater,
            (Some(r1), Some(ref r2)) => {
                match r1.cmp(r2) {
                    Less => return Less,
                    Greater => return Greater,
                    Equal => {} // Left-bounds are equal, we can't return yet.
                };
            }
        };

        // Both left-bounds are equal, we have to
        // compare the right-bounds as a tie-breaker.
        Self::cmp_endbound(&r1.1, &r2.1)
    }

    fn cmp_endbound(e1: &Bound<K>, e2: &Bound<K>) -> Ordering
    where
        K: Ord,
    {
        // Based on the encoding idea used in `cmp`.
        // Note that we have inversed the 2nd value in the tuple,
        // as the Included/Excluded rules are flipped for the upper bound.
        let e1 = match e1 {
            Included(x) => Some((x, 2)),
            Excluded(x) => Some((x, 1)),
            Unbounded => None,
        };
        let e2 = match e2 {
            Included(x) => Some((x, 2)),
            Excluded(x) => Some((x, 1)),
            Unbounded => None,
        };

        match (e1, e2) {
            (None, None) => Equal,
            (None, Some(_)) => Greater,
            (Some(_), None) => Less,
            (Some(r1), Some(ref r2)) => r1.cmp(r2),
        }
    }
}

/// An inorder interator through the interval tree.
pub struct IntervalTreeIter<'a, K> {
    to_visit: Vec<&'a Box<Node<K>>>,
    curr: &'a Option<Box<Node<K>>>,
}

impl<'a, K> Iterator for IntervalTreeIter<'a, K> {
    type Item = &'a Range<K>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.curr.is_none() && self.to_visit.is_empty() {
            return None;
        }

        while self.curr.is_some() {
            self.to_visit.push(self.curr.as_ref().unwrap());
            self.curr = &self.curr.as_ref().unwrap().left;
        }

        let visited = self.to_visit.pop();
        self.curr = &visited.as_ref().unwrap().right;
        Some(&visited.unwrap().key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, from_str, json, to_string};
    
    #[test]
    fn serialize_deserialize_identity() {
	let mut tree = IntervalTree::default();
	let serialized_empty_tree = to_string(&tree).unwrap();
	let deserialized_empty_tree = from_str(&serialized_empty_tree).unwrap();
	assert_eq!(tree, deserialized_empty_tree);

	tree.insert((Included(1), Excluded(3)));
	let serialized_tree = to_string(&tree).unwrap();
	let deserialized_tree = from_str(&serialized_tree).unwrap();
	assert_eq!(tree, deserialized_tree);
    }

    #[test]
    fn serialize() {
	let mut tree = IntervalTree::default();
	let serialized_empty_tree = to_string(&tree).unwrap();
	let deserialized_empty_value: Value = from_str(&serialized_empty_tree).unwrap();
	let expected_empty_value = json!({
	    "root": null,
	    "size": 0,
	});
	assert_eq!(expected_empty_value, deserialized_empty_value);

	tree.insert((Included(2), Included(4)));
	tree.insert((Included(1), Excluded(3)));
	
	let serialized_tree = to_string(&tree).unwrap();
	let deserialized_tree: Value = from_str(&serialized_tree).unwrap();
	let expected_value = json!({
	    "root": {
		"key": [
		    {"Included": 2},
		    {"Included": 4},
		],
		"left": {
		    "key": [
			{"Included": 1},
			{"Excluded": 3},
		    ],
		    "left": null,
		    "right": null,
		    "value": {"Excluded": 3},
		},
		"right": null,
		"value": {"Included": 4},
	    },
	    "size": 2,
	});
	assert_eq!(expected_value, deserialized_tree);
    }

    #[test]
    fn deserialize() {
	let mut expected_tree = IntervalTree::default();
	let empty_value = json!({
	    "root": null,
	    "size": 0,
	});
	let serialized_empty_value = empty_value.to_string();
	let deserialized_empty_tree = from_str(&serialized_empty_value).unwrap();
	assert_eq!(expected_tree, deserialized_empty_tree);

	expected_tree.insert((Included(2), Included(4)));
	expected_tree.insert((Included(1), Excluded(3)));
	let value = json!({
	    "root": {
		"key": [
		    {"Included": 2},
		    {"Included": 4},
		],
		"left": {
		    "key": [
			{"Included": 1},
			{"Excluded": 3},
		    ],
		    "left": null,
		    "right": null,
		    "value": {"Excluded": 3},
		},
		"right": null,
		"value": {"Included": 4},
	    },
	    "size": 2,
	});
	let serialized_value = value.to_string();
	let deserialized_tree = from_str(&serialized_value).unwrap();
	assert_eq!(expected_tree, deserialized_tree);
    }
    
    #[test]
    fn it_inserts_root() {
        let mut tree = IntervalTree::default();
        assert!(tree.root.is_none());

        let key = (Included(1), Included(3));

        tree.insert(key.clone());
        assert!(tree.root.is_some());
        assert_eq!(tree.root.as_ref().unwrap().key, key);
        assert_eq!(tree.root.as_ref().unwrap().value, key.1);
        assert!(tree.root.as_ref().unwrap().left.is_none());
        assert!(tree.root.as_ref().unwrap().right.is_none());
    }

    #[test]
    fn creates_from_iterator() {
        let ranges = vec![0..5, 6..10, 10..15];
        let interval_tree: IntervalTree<_> = ranges.into_iter().collect();

        assert_eq!(interval_tree.len(), 3);
    }

    #[test]
    fn creates_from_array() {
        let ranges = [0..5, 6..10, 10..15];
        let interval_tree = IntervalTree::from(ranges.clone());
        let other_interval_tree = ranges.into();

        assert_eq!(interval_tree, other_interval_tree);
        assert_eq!(interval_tree.len(), 3);
    }

    #[test]
    fn it_inserts_left_right_node() {
        let mut tree = IntervalTree::default();

        let root_key = (Included(2), Included(3));
        let left_key = (Included(0), Included(1));
        let left_right_key = (Excluded(1), Unbounded);

        tree.insert(root_key.clone());
        assert!(tree.root.is_some());
        assert!(tree.root.as_ref().unwrap().left.is_none());

        tree.insert(left_key.clone());
        assert!(tree.root.as_ref().unwrap().right.is_none());
        assert!(tree.root.as_ref().unwrap().left.is_some());
        assert_eq!(
            tree.root.as_ref().unwrap().left.as_ref().unwrap().value,
            left_key.1
        );

        tree.insert(left_right_key.clone());
        assert!(tree
            .root
            .as_ref()
            .unwrap()
            .left
            .as_ref()
            .unwrap()
            .right
            .is_some());
    }

    #[test]
    fn it_updates_value() {
        let mut tree = IntervalTree::default();

        let root_key = (Included(2), Included(3));
        let left_key = (Included(0), Included(1));
        let left_left_key = (Included(-5), Excluded(10));
        let right_key = (Excluded(3), Unbounded);

        tree.insert(root_key.clone());
        assert_eq!(tree.root.as_ref().unwrap().value, root_key.1);

        tree.insert(left_key.clone());
        assert_eq!(tree.root.as_ref().unwrap().value, root_key.1);
        assert!(tree.root.as_ref().unwrap().left.is_some());
        assert_eq!(
            tree.root.as_ref().unwrap().left.as_ref().unwrap().value,
            left_key.1
        );

        tree.insert(left_left_key.clone());
        assert_eq!(tree.root.as_ref().unwrap().value, left_left_key.1);
        assert_eq!(
            tree.root.as_ref().unwrap().left.as_ref().unwrap().value,
            left_left_key.1
        );
        assert!(tree
            .root
            .as_ref()
            .unwrap()
            .left
            .as_ref()
            .unwrap()
            .left
            .is_some());
        assert_eq!(
            tree.root
                .as_ref()
                .unwrap()
                .left
                .as_ref()
                .unwrap()
                .left
                .as_ref()
                .unwrap()
                .value,
            left_left_key.1
        );

        tree.insert(right_key.clone());
        assert_eq!(tree.root.as_ref().unwrap().value, right_key.1);
        assert!(tree.root.as_ref().unwrap().right.is_some());
        assert_eq!(
            tree.root.as_ref().unwrap().left.as_ref().unwrap().value,
            left_left_key.1
        );
        assert_eq!(
            tree.root.as_ref().unwrap().right.as_ref().unwrap().value,
            right_key.1
        );
    }

    #[test]
    fn cmp_works_as_expected() {
        let key0 = (Unbounded, Excluded(20));
        let key1 = (Included(1), Included(5));
        let key2 = (Included(1), Excluded(7));
        let key3 = (Included(1), Included(7));
        let key4 = (Excluded(5), Excluded(9));
        let key5 = (Included(7), Included(8));
        let key_str1 = (Included("abc"), Excluded("def"));
        let key_str2 = (Included("bbc"), Included("bde"));
        let key_str3: (_, Bound<&str>) = (Included("bbc"), Unbounded);

        assert_eq!(IntervalTree::cmp(&key1, &key1), Equal);
        assert_eq!(IntervalTree::cmp(&key1, &key2), Less);
        assert_eq!(IntervalTree::cmp(&key2, &key3), Less);
        assert_eq!(IntervalTree::cmp(&key0, &key1), Less);
        assert_eq!(IntervalTree::cmp(&key4, &key5), Less);
        assert_eq!(IntervalTree::cmp(&key_str1, &key_str2), Less);
        assert_eq!(IntervalTree::cmp(&key_str2, &key_str3), Less);
    }

    #[test]
    fn overlap_works_as_expected() {
        let mut tree = IntervalTree::default();

        let root_key = (Included(2), Included(3));
        let left_key = (Included(0), Included(1));
        let left_left_key = (Included(-5), Excluded(10));
        let right_key = (Excluded(3), Unbounded);

        tree.insert(root_key.clone());
        tree.insert(left_key.clone());
        assert_eq!(tree.get_interval_overlaps(&root_key), vec![&root_key]);

        tree.insert(left_left_key.clone());
        assert_eq!(
            tree.get_interval_overlaps(&(..)),
            vec![&left_left_key, &left_key, &root_key]
        );
        assert!(tree.get_interval_overlaps(&(100..)).is_empty());

        tree.insert(right_key);
        assert_eq!(
            tree.get_interval_overlaps(&root_key),
            vec![&left_left_key, &root_key]
        );
        assert_eq!(
            tree.get_interval_overlaps(&(..)),
            vec![&left_left_key, &left_key, &root_key, &right_key]
        );
        assert_eq!(tree.get_interval_overlaps(&(100..)), vec![&right_key]);
        assert_eq!(
            tree.get_interval_overlaps(&(3..10)),
            vec![&left_left_key, &root_key, &right_key]
        );
        assert_eq!(
            tree.get_interval_overlaps(&(Excluded(3), Excluded(10))),
            vec![&left_left_key, &right_key]
        );
        assert_eq!(
            tree.get_interval_overlaps(&(..2)),
            vec![&left_left_key, &left_key]
        );
        assert_eq!(
            tree.get_interval_overlaps(&(..=2)),
            vec![&left_left_key, &left_key, &root_key]
        );
        assert_eq!(
            tree.get_interval_overlaps(&(..=3)),
            vec![&left_left_key, &left_key, &root_key]
        );
    }

    #[test]
    fn difference_and_overlaps_with_tuple_works_as_expected() {
        let mut tree = IntervalTree::default();

        let root_key = (Included((1, 2)), Excluded((1, 4)));
        let right_key = (5, 10)..=(5, 20);

        tree.insert(root_key.clone());
        tree.insert(right_key);

        assert!(tree.get_interval_overlaps(&((2, 0)..=(2, 30))).is_empty());
        assert_eq!(
            tree.get_interval_overlaps(&((1, 3)..=(1, 5))),
            vec![&root_key]
        );
        assert_eq!(
            tree.get_interval_difference(&(Excluded((1, 1)), Included((1, 5)))),
            vec![
                (Excluded(&(1, 1)), Excluded(&(1, 2))),
                (Included(&(1, 4)), Included(&(1, 5)))
            ]
        );
    }

    #[test]
    fn difference_works_as_expected() {
        let mut tree = IntervalTree::default();

        let key1 = 2..10;
        let key2 = 4..=6;
        let key3 = (Excluded(10), Excluded(20));
        let key4 = (Excluded(30), Included(35));
        let key5 = 30..=40;
        let key6 = 30..=35;
        let key7 = (Excluded(45), Unbounded);
        let key8 = (Included(60), Included(70));

        tree.insert(key1);
        tree.insert(key2);
        tree.insert(key3);
        tree.insert(key4);
        tree.insert(key5);
        tree.insert(key6);
        tree.insert(key7);
        tree.insert(key8);

        assert_eq!(
            tree.get_interval_difference(&(Excluded(0), Included(100))),
            vec![
                (Excluded(&0), Excluded(&2)),
                (Included(&10), Included(&10)),
                (Included(&20), Excluded(&30)),
                (Excluded(&40), Included(&45))
            ]
        );
        assert_eq!(
            tree.get_interval_difference(&(19..=40)),
            vec![(Included(&20), Excluded(&30))]
        );
        assert_eq!(
            tree.get_interval_difference(&(20..=40)),
            vec![(Included(&20), Excluded(&30))]
        );
        assert_eq!(
            tree.get_interval_difference(&(20..=45)),
            vec![
                (Included(&20), Excluded(&30)),
                (Excluded(&40), Included(&45))
            ]
        );
        assert_eq!(
            tree.get_interval_difference(&(20..45)),
            vec![
                (Included(&20), Excluded(&30)),
                (Excluded(&40), Excluded(&45))
            ]
        );
        assert_eq!(
            tree.get_interval_difference(&(2..=10)),
            vec![(Included(&10), Included(&10))]
        );
    }

    #[test]
    fn consecutive_excluded_non_contiguous_difference_works_as_expected() {
        let mut tree = IntervalTree::default();

        let key1 = (Included(10), Excluded(20));
        let key2 = (Excluded(30), Excluded(40));

        tree.insert(key1);
        tree.insert(key2);

        assert_eq!(
            tree.get_interval_difference(&(0..=40)),
            vec![
                (Included(&0), Excluded(&10)),
                (Included(&20), Included(&30)),
                (Included(&40), Included(&40))
            ]
        );
    }

    #[test]
    fn get_interval_difference_str_works_as_expected() {
        let mut tree: IntervalTree<&str> = IntervalTree::default();

        let key1 = (Included("a"), Excluded("h"));
        let key2 = (Excluded("M"), Excluded("O"));

        tree.insert(key1.clone());
        tree.insert(key2);

        assert!(tree.get_interval_difference(&("a".."h")).is_empty());
        assert_eq!(
            tree.get_interval_difference(&("M"..="P")),
            vec![
                (Included(&"M"), Included(&"M")),
                (Included(&"O"), Included(&"P"))
            ]
        );

        let not_covered_range = "h".."k";
        assert_eq!(
            tree.get_interval_difference(&not_covered_range),
            vec![(
                not_covered_range.start_bound(),
                not_covered_range.end_bound()
            )]
        );
    }

    #[test]
    fn contains_point_works_as_expected() {
        let mut tree = IntervalTree::default();

        let key1 = (Included(10), Excluded(20));
        let key2 = (Excluded(30), Excluded(40));
        let key3 = 40..;

        tree.insert(key1);
        tree.insert(key2);
        tree.insert(key3);

        assert!(tree.contains_point(&10));
        assert!(!tree.contains_point(&20));
        assert!(tree.contains_point(&40));
        assert!(tree.contains_point(&100));
    }

    #[test]
    fn contains_string_point_works_as_expected() {
        let mut tree = IntervalTree::default();

        let key1 = String::from("a")..String::from("h");
        let key2 = (Excluded(String::from("M")), Excluded(String::from("O")));

        tree.insert(key1);
        tree.insert(key2);

        assert!(tree.contains_point("b"));
        assert!(!tree.contains_point("n"));
        assert!(tree.contains_point(&String::from("N")));
        assert!(tree.contains_point("g"));
    }

    #[test]
    fn contains_str_point_works_as_expected() {
        let mut tree: IntervalTree<&str> = IntervalTree::default();

        let key1 = "a".."h";
        let key2 = (Excluded("M"), Excluded("O"));

        tree.insert(key1);
        tree.insert(key2);

        assert!(tree.contains_point("b"));
        assert!(!tree.contains_point("n"));
        assert!(tree.contains_point(&"N"));
        assert!(tree.contains_point("g"));
    }

    #[test]
    fn contains_works_as_expected() {
        let mut tree = IntervalTree::default();

        let key1 = (Included(10), Excluded(20));
        let key2 = (Excluded(30), Excluded(40));
        let key3 = 40..;

        tree.insert(key1.clone());
        tree.insert(key2.clone());
        tree.insert(key3.clone());

        assert!(tree.contains_interval(&key1));
        assert!(!tree.contains_interval(&(Included(&10), Included(&20))));
        assert!(!tree.contains_interval(&(..=0)));
        assert!(tree.contains_interval(&(Included(35), Included(37))));
    }

    #[test]
    fn contains_str_works_as_expected() {
        let mut tree: IntervalTree<&str> = IntervalTree::default();

        let key1 = "a".."h";
        let key2 = (Excluded("M"), Excluded("O"));

        tree.insert(key1.clone());
        tree.insert(key2);

        assert!(tree.contains_interval(&("a".."h")));
        assert!(tree.contains_interval(&("N"..="N")));
        assert!(tree.contains_interval::<&str, _>(&key1));
        assert!(!tree.contains_interval(&("N"..="O")));
    }

    #[test]
    fn iter_works_as_expected() {
        let mut tree = IntervalTree::default();

        assert_eq!(tree.iter().next(), None);

        let key1 = (Included(10), Excluded(20));
        let key2 = (Included(40), Unbounded);
        let key3 = (Excluded(30), Excluded(40));
        let key4 = (Unbounded, Included(50));
        let key5 = (Excluded(-10), Included(-5));
        let key6 = (Included(-10), Included(-4));

        tree.insert(key1.clone());
        tree.insert(key2.clone());
        tree.insert(key3.clone());
        tree.insert(key4.clone());
        tree.insert(key5.clone());
        tree.insert(key6.clone());

        let inorder = vec![&key4, &key6, &key5, &key1, &key3, &key2];
        for (idx, interval) in tree.iter().enumerate() {
            assert_eq!(interval, inorder[idx]);
        }

        assert_eq!(tree.iter().count(), inorder.len());
    }

    #[test]
    fn remove_random_leaf_empty_tree_works_as_expected() {
        let mut tree: IntervalTree<i32> = IntervalTree::default();

        assert_eq!(tree.remove_random_leaf(), None);
    }

    #[test]
    fn remove_random_leaf_one_node_tree_works_as_expected() {
        let mut tree = IntervalTree::default();

        let key1 = (Included(10), Excluded(20));
        tree.insert(key1.clone());

        let deleted = tree.remove_random_leaf();
        assert!(deleted.is_some());
        assert_eq!(deleted.unwrap(), key1);

        assert!(tree.remove_random_leaf().is_none());
    }

    #[test]
    fn remove_random_leaf_works_as_expected() {
        let mut tree = IntervalTree::default();

        let key1 = (Included(16), Unbounded);
        let key2 = (Included(8), Excluded(9));
        let key3 = (Included(5), Excluded(8));
        let key4 = (Excluded(15), Included(23));
        let key5 = (Included(0), Included(3));
        let key6 = (Included(13), Excluded(26));

        tree.insert(key1.clone());
        tree.insert(key2.clone());
        tree.insert(key3.clone());
        tree.insert(key4.clone());
        tree.insert(key5.clone());
        tree.insert(key6.clone());

        let mut tree_deleted_key5 = IntervalTree::default();

        let key1_deleted5 = (Included(16), Unbounded);
        let key2_deleted5 = (Included(8), Excluded(9));
        let key3_deleted5 = (Included(5), Excluded(8));
        let key4_deleted5 = (Excluded(15), Included(23));
        let key6_deleted5 = (Included(13), Excluded(26));

        tree_deleted_key5.insert(key1_deleted5.clone());
        tree_deleted_key5.insert(key2_deleted5.clone());
        tree_deleted_key5.insert(key3_deleted5.clone());
        tree_deleted_key5.insert(key4_deleted5.clone());
        tree_deleted_key5.insert(key6_deleted5.clone());

        let mut tree_deleted_key6 = IntervalTree::default();

        let key1_deleted6 = (Included(16), Unbounded);
        let key2_deleted6 = (Included(8), Excluded(9));
        let key3_deleted6 = (Included(5), Excluded(8));
        let key4_deleted6 = (Excluded(15), Included(23));
        let key5_deleted6 = (Included(0), Included(3));

        tree_deleted_key6.insert(key1_deleted6.clone());
        tree_deleted_key6.insert(key2_deleted6.clone());
        tree_deleted_key6.insert(key3_deleted6.clone());
        tree_deleted_key6.insert(key4_deleted6.clone());
        tree_deleted_key6.insert(key5_deleted6.clone());

        use std::collections::HashSet;
        let mut all_deleted = HashSet::new();
        let num_of_leaves = 2; // Key5 & Key6

        // This loop makes sure that the deletion is random.
        // We delete and reinsert leaves until we have deleted
        // all possible leaves in the tree.
        while all_deleted.len() < num_of_leaves {
            let deleted = tree.remove_random_leaf();
            assert!(deleted.is_some());
            let deleted = deleted.unwrap();

            // Check that the new tree has the right shape,
            // and that the value stored in the various nodes are
            // correctly updated following the removal of a leaf.
            if deleted == key5 {
                assert_eq!(tree, tree_deleted_key5);
            } else if deleted == key6 {
                assert_eq!(tree, tree_deleted_key6);
            } else {
                unreachable!();
            }

            // Keep track of deleted nodes, and reinsert the
            // deleted node in the tree so we come back to
            // the initial state every iteration.
            all_deleted.insert(deleted.clone());
            tree.insert(deleted);
        }
    }

    #[test]
    fn len_and_is_empty_works_as_expected() {
        let mut tree = IntervalTree::default();

        assert_eq!(tree.len(), 0);
        assert!(tree.is_empty());

        let key1 = (Included(16), Unbounded);
        let key2 = (Included(8), Excluded(9));

        tree.insert(key1);

        assert_eq!(tree.len(), 1);
        assert!(!tree.is_empty());

        tree.insert(key2);

        assert_eq!(tree.len(), 2);
        assert!(!tree.is_empty());

        tree.remove_random_leaf();

        assert_eq!(tree.len(), 1);
        assert!(!tree.is_empty());

        tree.remove_random_leaf();

        assert_eq!(tree.len(), 0);
        assert!(tree.is_empty());
    }

    #[test]
    fn clear_works_as_expected() {
        let mut tree = IntervalTree::default();

        tree.clear();

        let key1 = (Included(16), Unbounded);
        let key2 = (Included(8), Excluded(9));

        tree.insert(key1.clone());
        tree.insert(key2.clone());

        assert_eq!(tree.len(), 2);

        tree.clear();

        assert!(tree.is_empty());
        assert_eq!(tree.root, None);
    }
}
