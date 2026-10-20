use crate::geometry::{Rect, Size};

pub(crate) const SCREEN_MARGIN: i32 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Placement {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Placement {
    const PREFERENCE: [Self; 4] = [
        Self::TopLeft,
        Self::TopRight,
        Self::BottomLeft,
        Self::BottomRight,
    ];

    pub(crate) fn choose(clippy: Rect, balloon: Size, screen: Rect) -> (Self, Rect) {
        let area = screen.inset(SCREEN_MARGIN);
        Self::PREFERENCE
            .into_iter()
            .map(|placement| (placement, placement.frame(clippy, balloon)))
            .min_by_key(|(_, frame)| frame.overflow(area))
            .expect("there should always be placements to choose from")
    }

    pub(crate) fn is_above(self) -> bool {
        matches!(self, Self::TopLeft | Self::TopRight)
    }

    pub(super) fn extends_left(self) -> bool {
        matches!(self, Self::TopLeft | Self::BottomLeft)
    }

    fn frame(self, clippy: Rect, balloon: Size) -> Rect {
        let x = if self.extends_left() {
            clippy.right() - balloon.width
        } else {
            clippy.x
        };
        let y = if self.is_above() {
            clippy.y - balloon.height
        } else {
            clippy.bottom()
        };
        Rect::new(x, y, balloon.width, balloon.height)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Rect = Rect::new(0, 0, 1000, 800);
    const BALLOON: Size = Size::new(200, 100);

    #[test]
    fn prefers_above_and_to_the_left() {
        let clippy = Rect::new(700, 600, 124, 93);
        assert_eq!(
            Placement::choose(clippy, BALLOON, SCREEN),
            (Placement::TopLeft, Rect::new(624, 500, 200, 100))
        );
    }

    #[test]
    fn flips_to_the_right_near_the_left_edge() {
        let clippy = Rect::new(10, 600, 124, 93);
        assert_eq!(
            Placement::choose(clippy, BALLOON, SCREEN).0,
            Placement::TopRight
        );
    }

    #[test]
    fn stays_above_when_dragged_over_the_dock() {
        let clippy = Rect::new(700, 798, 124, 93);
        assert_eq!(
            Placement::choose(clippy, BALLOON, SCREEN).0,
            Placement::TopLeft
        );
    }

    #[test]
    fn extends_left_when_pushed_past_the_right_edge() {
        let clippy = Rect::new(940, 760, 124, 93);
        assert_eq!(
            Placement::choose(clippy, BALLOON, SCREEN).0,
            Placement::TopLeft
        );
    }

    #[test]
    fn drops_below_near_the_top_edge() {
        let clippy = Rect::new(700, 20, 124, 93);
        assert_eq!(
            Placement::choose(clippy, BALLOON, SCREEN),
            (Placement::BottomLeft, Rect::new(624, 113, 200, 100))
        );
    }
}
