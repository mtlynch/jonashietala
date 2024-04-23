use btree_range_map::RangeMap;
use std::{collections::HashMap, ops::Range};

// Neovim uses (1,0)-indexing.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct NeovimPos {
    pub row: usize,
    pub col: usize,
}

impl NeovimPos {
    pub fn new(row: usize, col: usize) -> NeovimPos {
        assert!(row > 0, "Can't use 0 for a Neovim row pos");
        Self { row, col }
    }
}

impl PartialEq<(usize, usize)> for NeovimPos {
    fn eq(&self, other: &(usize, usize)) -> bool {
        self.row == other.0 && self.col == other.1
    }
}

// A range used to send to Neovim.
// Note that it's range inclusive!
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct NeovimRange {
    pub start: NeovimPos,
    pub end: NeovimPos,
}

impl NeovimRange {
    pub fn new(start: (usize, usize), end: (usize, usize)) -> Self {
        Self {
            start: NeovimPos::new(start.0, start.1),
            end: NeovimPos::new(end.0, end.1),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Heading {
    pub id: String,
    pub level: u16,
    pub content: String,
    pub range: NeovimRange,
    pub char_range: Range<usize>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct LinkDef {
    pub label: String,
    pub url: String,
    pub range: NeovimRange,
    pub char_range: Range<usize>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum LinkRef {
    Inline(String),
    Reference { label: String, url: String },
    Email(String),
    AutoLink(String),
    Unresolved(String),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Link {
    pub link_ref: LinkRef,
    pub range: NeovimRange,
    pub char_range: Range<usize>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ImgRef {
    Inline(String),
    Reference { label: String, url: String },
    Unresolved(String),
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Img {
    pub link_ref: ImgRef,
    pub range: NeovimRange,
    pub char_range: Range<usize>,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum ElementInfo {
    Link(Link),
    LinkDef(LinkDef),
    Heading(Heading),
    Img(Img),
}

type LinkLabel = String;
type HeadingId = String;

#[derive(Debug)]
pub struct MarkupLookup {
    // Element lookup by character position.
    pub char_pos_to_element: RangeMap<usize, ElementInfo>,

    // Element lookup by id or type.
    pub link_defs: HashMap<LinkLabel, Vec<LinkDef>>,
    pub headings: HashMap<HeadingId, Vec<Heading>>,

    // Position translations.
    prev_line_size_sum: Vec<usize>,
    // Line calculation offset, to handle frontmatter that isn't
    // included in markup registration.
    line_calc_offset: usize,
}

impl MarkupLookup {
    pub fn new(source: &str, line_calc_offset: usize) -> Self {
        let mut line_size_sum = Vec::new();

        let mut sum = 0;
        for line in source.lines() {
            // Include the newline.
            // I don't know how to handle carriage returns nor do I care.
            // Neovim counts columns using bytes, not character!
            let count = line.bytes().count() + 1;
            line_size_sum.push(sum);
            sum += count;
        }
        line_size_sum.push(sum);

        Self {
            char_pos_to_element: RangeMap::new(),
            link_defs: HashMap::new(),
            headings: HashMap::new(),
            prev_line_size_sum: line_size_sum,
            line_calc_offset,
        }
    }

    pub fn in_frontmatter(&self, row: usize) -> bool {
        row < self.line_calc_offset
    }

    pub fn element_at(&self, row: usize, col: usize) -> Option<&ElementInfo> {
        self.char_pos_to_element
            .get(self.row_col_to_char_pos(row, col)?)
    }

    pub fn neovim_range(&self, range: &Range<usize>) -> NeovimRange {
        NeovimRange {
            start: self.char_pos_to_row_col(range.start),
            // A Range excludes `end`, but NeovimRange is inclusive.
            end: self.char_pos_to_row_col(range.end - 1),
        }
    }

    fn char_pos_to_row_col(&self, pos: usize) -> NeovimPos {
        match self.prev_line_size_sum.binary_search(&pos) {
            // Found an exact match
            Ok(row) => NeovimPos::new(row + 1 + self.line_calc_offset, 0),
            // An error means we could insert it here sorted,
            // but we'll use it to calculate the remaining chars.
            Err(row) => NeovimPos::new(
                row + self.line_calc_offset,
                pos - self.prev_line_size_sum[row - 1],
            ),
        }
    }

    fn row_col_to_char_pos(&self, row: usize, col: usize) -> Option<usize> {
        if row == 0 || self.line_calc_offset > row - 1 {
            return None;
        }
        let row_check = row - 1 - self.line_calc_offset;
        if row_check + 1 >= self.prev_line_size_sum.len() {
            return None;
        }

        Some(self.prev_line_size_sum[row_check] + col)
    }

    pub fn at_pos(&self, pos: usize) -> Option<&ElementInfo> {
        self.char_pos_to_element.get(pos)
    }

    pub fn insert_heading(&mut self, heading: Heading) {
        self.headings
            .entry(heading.id.clone())
            .and_modify(|hs| hs.push(heading.clone()))
            .or_insert_with(|| vec![heading.clone()]);

        self.char_pos_to_element
            .insert(heading.char_range.clone(), ElementInfo::Heading(heading));
    }

    pub fn insert_img(&mut self, img: Img) {
        self.char_pos_to_element
            .insert(img.char_range.clone(), ElementInfo::Img(img));
    }

    pub fn insert_link(&mut self, link: Link) {
        self.char_pos_to_element
            .insert(link.char_range.clone(), ElementInfo::Link(link));
    }

    pub fn insert_link_def(&mut self, link_def: LinkDef) {
        self.link_defs
            .entry(link_def.label.clone())
            .and_modify(|hs| hs.push(link_def.clone()))
            .or_insert_with(|| vec![link_def.clone()]);

        self.char_pos_to_element
            .insert(link_def.char_range.clone(), ElementInfo::LinkDef(link_def));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_position_transforms() {
        let lookup = MarkupLookup::new(
            "012345

01234
0",
            0,
        );

        // Neovim uses (1,0)-indexing where rows start at 1
        // and columns at 0 when we use the
        // `nvim_win_get_cursor` and `nvim_win_set_cursor` functions.
        assert_eq!(lookup.char_pos_to_row_col(0), (1, 0));
        assert_eq!(lookup.char_pos_to_row_col(1), (1, 1));
        assert_eq!(lookup.char_pos_to_row_col(2), (1, 2));
        assert_eq!(lookup.char_pos_to_row_col(3), (1, 3));
        assert_eq!(lookup.char_pos_to_row_col(4), (1, 4));
        assert_eq!(lookup.char_pos_to_row_col(5), (1, 5));
        assert_eq!(lookup.char_pos_to_row_col(6), (1, 6));

        assert_eq!(lookup.char_pos_to_row_col(7), (2, 0));

        assert_eq!(lookup.char_pos_to_row_col(8), (3, 0));
        assert_eq!(lookup.char_pos_to_row_col(9), (3, 1));
        assert_eq!(lookup.char_pos_to_row_col(10), (3, 2));
        assert_eq!(lookup.char_pos_to_row_col(11), (3, 3));
        assert_eq!(lookup.char_pos_to_row_col(12), (3, 4));
        assert_eq!(lookup.char_pos_to_row_col(13), (3, 5));

        assert_eq!(lookup.char_pos_to_row_col(14), (4, 0));
        assert_eq!(lookup.char_pos_to_row_col(15), (4, 1));

        assert_eq!(lookup.row_col_to_char_pos(0, 0), None);

        assert_eq!(lookup.row_col_to_char_pos(1, 0), Some(0));
        assert_eq!(lookup.row_col_to_char_pos(1, 1), Some(1));
        assert_eq!(lookup.row_col_to_char_pos(1, 2), Some(2));
        assert_eq!(lookup.row_col_to_char_pos(1, 3), Some(3));
        assert_eq!(lookup.row_col_to_char_pos(1, 4), Some(4));
        assert_eq!(lookup.row_col_to_char_pos(1, 5), Some(5));
        assert_eq!(lookup.row_col_to_char_pos(1, 6), Some(6));

        assert_eq!(lookup.row_col_to_char_pos(2, 0), Some(7));

        assert_eq!(lookup.row_col_to_char_pos(3, 0), Some(8));
        assert_eq!(lookup.row_col_to_char_pos(3, 1), Some(9));
        assert_eq!(lookup.row_col_to_char_pos(3, 2), Some(10));
        assert_eq!(lookup.row_col_to_char_pos(3, 3), Some(11));
        assert_eq!(lookup.row_col_to_char_pos(3, 4), Some(12));
        assert_eq!(lookup.row_col_to_char_pos(3, 5), Some(13));

        assert_eq!(lookup.row_col_to_char_pos(4, 0), Some(14));
        assert_eq!(lookup.row_col_to_char_pos(4, 1), Some(15));

        assert_eq!(lookup.row_col_to_char_pos(5, 0), None);
    }

    #[test]
    fn test_lookup_position_offset() {
        let lookup = MarkupLookup::new("012345", 4);

        assert_eq!(lookup.char_pos_to_row_col(0), (5, 0));
        assert_eq!(lookup.char_pos_to_row_col(1), (5, 1));
        assert_eq!(lookup.char_pos_to_row_col(2), (5, 2));
        assert_eq!(lookup.char_pos_to_row_col(3), (5, 3));
        assert_eq!(lookup.char_pos_to_row_col(4), (5, 4));
        assert_eq!(lookup.char_pos_to_row_col(5), (5, 5));
        assert_eq!(lookup.char_pos_to_row_col(6), (5, 6));

        assert_eq!(lookup.row_col_to_char_pos(0, 0), None);
        assert_eq!(lookup.row_col_to_char_pos(1, 0), None);
        assert_eq!(lookup.row_col_to_char_pos(2, 0), None);
        assert_eq!(lookup.row_col_to_char_pos(3, 0), None);
        assert_eq!(lookup.row_col_to_char_pos(4, 0), None);

        assert_eq!(lookup.row_col_to_char_pos(5, 0), Some(0));
        assert_eq!(lookup.row_col_to_char_pos(5, 1), Some(1));
        assert_eq!(lookup.row_col_to_char_pos(5, 2), Some(2));
        assert_eq!(lookup.row_col_to_char_pos(5, 3), Some(3));
        assert_eq!(lookup.row_col_to_char_pos(5, 4), Some(4));
        assert_eq!(lookup.row_col_to_char_pos(5, 5), Some(5));
        assert_eq!(lookup.row_col_to_char_pos(5, 6), Some(6));
    }

    #[test]
    fn test_lookup_lines_counts_bytes() {
        let lookup = MarkupLookup::new("’", 5);
        assert_eq!(lookup.prev_line_size_sum, vec![0, 4]); // 3 for ’ + newline
    }

    #[test]
    fn test_neovim_range() {
        let lookup = MarkupLookup::new(
            "first line
second line
third line",
            0,
        );

        assert_eq!(
            lookup.neovim_range(&(0..1)),
            NeovimRange::new((1, 0), (1, 0))
        );
        assert_eq!(
            lookup.neovim_range(&(0..5)),
            NeovimRange::new((1, 0), (1, 4))
        );
        assert_eq!(
            lookup.neovim_range(&(7..25)),
            NeovimRange::new((1, 7), (3, 1))
        );
    }
}
