//! The Harbour workspace sidebar: [`FloatingSidebar`] with its cluster card,
//! kind filter, captions, kind list and footer.

mod floating;

pub use floating::{
    ClusterCard, FloatingSidebar, SideItem, kind_filter, kind_filter_with_key, side_caption,
    side_footer,
};
