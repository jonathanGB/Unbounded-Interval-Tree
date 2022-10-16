use std::fmt;
use std::ops::Bound;
use std::ops::Bound::*;
#[cfg(feature="serde")]
use serde::{Serialize, Deserialize};

pub(crate) type Range<K> = (Bound<K>, Bound<K>);

#[cfg_attr(feature="serde", derive(Serialize, Deserialize))]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Node<K> {
    pub key: Range<K>,
    pub value: Bound<K>, // Max end-point.
    pub left: Option<Box<Node<K>>>,
    pub right: Option<Box<Node<K>>>,
}

impl<K> fmt::Display for Node<K>
where
    K: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let start = match self.key.0 {
            Included(ref x) => format!("[{}", x),
            Excluded(ref x) => format!("]{}", x),
            Unbounded => String::from("]-∞"),
        };
        let end = match self.key.1 {
            Included(ref x) => format!("{}]", x),
            Excluded(ref x) => format!("{}[", x),
            Unbounded => format!("∞["),
        };
        let value = match self.value {
            Included(ref x) => format!("{}]", x),
            Excluded(ref x) => format!("{}[", x),
            Unbounded => String::from("∞"),
        };

        if self.left.is_none() && self.right.is_none() {
            write!(f, " {{ {},{} ({}) }} ", start, end, value)
        } else if self.left.is_none() {
            write!(
                f,
                " {{ {},{} ({}) right:{}}} ",
                start,
                end,
                value,
                self.right.as_ref().unwrap()
            )
        } else if self.right.is_none() {
            write!(
                f,
                " {{ {},{} ({}) left:{}}} ",
                start,
                end,
                value,
                self.left.as_ref().unwrap()
            )
        } else {
            write!(
                f,
                " {{ {},{} ({}) left:{}right:{}}} ",
                start,
                end,
                value,
                self.left.as_ref().unwrap(),
                self.right.as_ref().unwrap()
            )
        }
    }
}

impl<K> Node<K> {
    pub fn new(range: Range<K>) -> Node<K>
    where
        K: Clone,
    {
        let max = range.1.clone();

        Node {
            key: range,
            value: max,
            left: None,
            right: None,
        }
    }

    pub fn is_leaf(&self) -> bool {
        self.left.is_none() && self.right.is_none()
    }

    pub fn maybe_update_value(&mut self, inserted_max: &Bound<K>)
    where
        K: PartialOrd + Clone,
    {
        let self_max_q = match &self.value {
            Included(x) => Some((x, 2)),
            Excluded(x) => Some((x, 1)),
            Unbounded => None,
        };
        let inserted_max_q = match inserted_max {
            Included(x) => Some((x, 2)),
            Excluded(x) => Some((x, 1)),
            Unbounded => None,
        };
        match (self_max_q, inserted_max_q) {
            (None, _) => {}
            (_, None) => self.value = Unbounded,
            (Some(self_max_q), Some(inserted_max_q)) => {
                if self_max_q < inserted_max_q {
                    self.value = inserted_max.clone();
                }
            }
        };
    }
}
