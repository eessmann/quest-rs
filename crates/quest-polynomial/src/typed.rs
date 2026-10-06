//! Compatibility re-exports from the maintained `MathCore` ordered expression engine.
pub use mathcore::typed::*;

/// Construct a concrete expression without an interpreted tree or runtime
/// dispatch. Repeated variable leaves are Copy; owned constants are moved once.
#[macro_export]
macro_rules! function {
    (|$($x:ident),+| [$($expression:expr),+ $(,)?]) => {{
        $crate::function!(@bind 0usize; $($x),+);
        $crate::System::<_,{$crate::function!(@count $($x),+)},{$crate::function!(@outputs $($expression),+)}>::new(($($crate::typed::capture($expression),)+))
    }};
    (@outputs $head:expr $(,$tail:expr)*) => {1usize $(+ $crate::function!(@output $tail))*};
    (@output $x:expr) => {1usize};
    (|$x:ident| $expression:expr) => {{
        let $x=$crate::typed::variable::<0>();
        $crate::Function::<_,1>::new($crate::typed::capture($expression))
    }};
    (|$($x:ident),+| $expression:expr) => {{
        $crate::function!(@bind 0usize; $($x),+);
        $crate::Function::<_,{$crate::function!(@count $($x),+)}>::new($crate::typed::capture($expression))
    }};
    (@bind $index:expr; $head:ident $(,$tail:ident)*) => {
        let $head=$crate::typed::variable::<{$index}>();
        $crate::function!(@bind ($index+1usize); $($tail),*);
    };
    (@bind $index:expr;) => {};
    (@count $head:ident $(,$tail:ident)*) => {1usize $(+ $crate::function!(@one $tail))*};
    (@one $x:ident) => {1usize};
}
