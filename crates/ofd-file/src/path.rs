//! Parser for OFD `PathObject/AbbreviatedData` (GB/T 33190-2016, 9.1).
//!
//! Grammar: whitespace-separated tokens, each command letter followed by a
//! known number of numeric operands.
//!
//! ```text
//! S x y           ; start point (same as M)
//! M x y           ; move to
//! L x y           ; line to
//! Q x1 y1 x y     ; quadratic Bézier
//! B x1 y1 x2 y2 x y  ; cubic Bézier (equivalent to SVG C)
//! A rx ry xr large sweep x y  ; arc
//! C               ; close path
//! ```

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PathCmd {
    MoveTo { x: f64, y: f64 },
    LineTo { x: f64, y: f64 },
    QuadTo { x1: f64, y1: f64, x: f64, y: f64 },
    CurveTo { x1: f64, y1: f64, x2: f64, y2: f64, x: f64, y: f64 },
    ArcTo { rx: f64, ry: f64, x_rot: f64, large: bool, sweep: bool, x: f64, y: f64 },
    Close,
}

/// Parse an `AbbreviatedData` string into a list of path commands.
/// Unknown tokens are skipped; truncated commands are dropped.
pub fn parse_abbreviated(data: &str) -> Vec<PathCmd> {
    let mut out = Vec::new();
    let mut it = data.split_whitespace().peekable();

    // Read a numeric operand.
    fn num<'a, I: Iterator<Item = &'a str>>(it: &mut std::iter::Peekable<I>) -> Option<f64> {
        it.next().and_then(|s| s.parse::<f64>().ok())
    }

    while let Some(tok) = it.next() {
        match tok {
            "S" | "M" => {
                if let (Some(x), Some(y)) = (num(&mut it), num(&mut it)) {
                    out.push(PathCmd::MoveTo { x, y });
                }
            }
            "L" => {
                if let (Some(x), Some(y)) = (num(&mut it), num(&mut it)) {
                    out.push(PathCmd::LineTo { x, y });
                }
            }
            "Q" => {
                if let (Some(x1), Some(y1), Some(x), Some(y)) =
                    (num(&mut it), num(&mut it), num(&mut it), num(&mut it))
                {
                    out.push(PathCmd::QuadTo { x1, y1, x, y });
                }
            }
            "B" => {
                if let (Some(x1), Some(y1), Some(x2), Some(y2), Some(x), Some(y)) = (
                    num(&mut it),
                    num(&mut it),
                    num(&mut it),
                    num(&mut it),
                    num(&mut it),
                    num(&mut it),
                ) {
                    out.push(PathCmd::CurveTo { x1, y1, x2, y2, x, y });
                }
            }
            "A" => {
                let rx = num(&mut it);
                let ry = num(&mut it);
                let xr = num(&mut it);
                let large = num(&mut it);
                let sweep = num(&mut it);
                let x = num(&mut it);
                let y = num(&mut it);
                if let (Some(rx), Some(ry), Some(xr), Some(large), Some(sweep), Some(x), Some(y)) =
                    (rx, ry, xr, large, sweep, x, y)
                {
                    out.push(PathCmd::ArcTo {
                        rx,
                        ry,
                        x_rot: xr,
                        large: large != 0.0,
                        sweep: sweep != 0.0,
                        x,
                        y,
                    });
                }
            }
            "C" => out.push(PathCmd::Close),
            _ => { /* skip unknown */ }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple() {
        let p = parse_abbreviated("M 0 0.15 L 6.60 0.15");
        assert_eq!(
            p,
            vec![
                PathCmd::MoveTo { x: 0.0, y: 0.15 },
                PathCmd::LineTo { x: 6.60, y: 0.15 },
            ]
        );
    }

    #[test]
    fn parse_cubic_and_close() {
        let p = parse_abbreviated("M 0 0 B 1 1 2 2 3 3 C");
        assert_eq!(
            p,
            vec![
                PathCmd::MoveTo { x: 0.0, y: 0.0 },
                PathCmd::CurveTo { x1: 1.0, y1: 1.0, x2: 2.0, y2: 2.0, x: 3.0, y: 3.0 },
                PathCmd::Close,
            ]
        );
    }
}
