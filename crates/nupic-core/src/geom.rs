//! Geometric primitives for sizes, positions, and rectangles.
//!
//! Reserved for use across resize / fit / circle / watermark / bbox.

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

impl Point {
    pub const ORIGIN: Self = Self { x: 0, y: 0 };

    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub const ZERO: Self = Self { width: 0, height: 0 };

    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub fn area(self) -> u64 {
        u64::from(self.width) * u64::from(self.height)
    }

    pub fn aspect(self) -> f64 {
        f64::from(self.width) / f64::from(self.height.max(1))
    }

    pub fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rect {
    pub origin: Point,
    pub size: Size,
}

impl Rect {
    pub const fn new(origin: Point, size: Size) -> Self {
        Self { origin, size }
    }

    pub const fn from_xywh(x: i32, y: i32, width: u32, height: u32) -> Self {
        Self::new(Point::new(x, y), Size::new(width, height))
    }

    pub fn left(self) -> i32 {
        self.origin.x
    }

    pub fn top(self) -> i32 {
        self.origin.y
    }

    pub fn right(self) -> i32 {
        self.origin.x.saturating_add(self.size.width as i32)
    }

    pub fn bottom(self) -> i32 {
        self.origin.y.saturating_add(self.size.height as i32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn size_zero_is_empty() {
        assert_eq!(Size::ZERO, Size::new(0, 0));
        assert!(Size::ZERO.is_empty());
        assert!(Size::new(0, 100).is_empty());
        assert!(Size::new(100, 0).is_empty());
        assert!(!Size::new(1, 1).is_empty());
    }

    #[test]
    fn size_area_is_width_times_height() {
        assert_eq!(Size::new(800, 600).area(), 480_000);
        assert_eq!(Size::new(0, 100).area(), 0);
        // overflow-safe: stays in u64
        assert_eq!(Size::new(u32::MAX, 2).area(), u64::from(u32::MAX) * 2);
    }

    #[test]
    fn size_aspect_is_w_over_h() {
        assert!((Size::new(800, 600).aspect() - 4.0 / 3.0).abs() < 1e-9);
        assert!((Size::new(16, 9).aspect() - 16.0 / 9.0).abs() < 1e-9);
    }

    #[test]
    fn point_origin_constant() {
        assert_eq!(Point::ORIGIN, Point::new(0, 0));
    }

    #[test]
    fn rect_bounds_compute_correctly() {
        let r = Rect::from_xywh(5, 10, 20, 30);
        assert_eq!(r.left(), 5);
        assert_eq!(r.top(), 10);
        assert_eq!(r.right(), 25);
        assert_eq!(r.bottom(), 40);
    }
}
