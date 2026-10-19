//! Owned BGR8 frames and the image preprocessing boundary used by OCR.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BgrFrame {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
}
impl BgrFrame {
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Self, String> {
        let len = (width as usize)
            .checked_mul(height as usize)
            .and_then(|n| n.checked_mul(3));
        if width == 0 || height == 0 || len != Some(pixels.len()) {
            return Err("invalid BGR frame dimensions or byte count".into());
        }
        Ok(Self {
            width,
            height,
            pixels,
        })
    }
    pub fn width(&self) -> u32 {
        self.width
    }
    pub fn height(&self) -> u32 {
        self.height
    }
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
    pub fn to_rgb(&self) -> Vec<u8> {
        self.pixels
            .chunks_exact(3)
            .flat_map(|p| [p[2], p[1], p[0]])
            .collect()
    }
    /// OpenCV BGR2GRAY uses 15-bit fixed coefficients, not floating luma.
    pub fn to_gray(&self) -> Vec<u8> {
        self.pixels
            .chunks_exact(3)
            .map(|p| {
                ((p[0] as u32 * 3735 + p[1] as u32 * 19235 + p[2] as u32 * 9798 + 16384) >> 15)
                    as u8
            })
            .collect()
    }
    /// OpenCV's uint8 HSV: hue in [0,179], 12-bit rounded reciprocal tables.
    pub fn to_hsv(&self) -> Vec<u8> {
        let mut sdiv = [0i32; 256];
        let mut hdiv = [0i32; 256];
        for i in 1..256 {
            sdiv[i] = ((255 << 12) as f64 / i as f64).round_ties_even() as i32;
            hdiv[i] = ((180 << 12) as f64 / (6 * i) as f64).round_ties_even() as i32;
        }
        self.pixels
            .chunks_exact(3)
            .flat_map(|p| {
                let (b, g, r) = (p[0] as i32, p[1] as i32, p[2] as i32);
                let v = b.max(g).max(r);
                let diff = v - b.min(g).min(r);
                let s = (diff * sdiv[v as usize] + 2048) >> 12;
                let h = if v == r {
                    g - b
                } else if v == g {
                    b - r + 2 * diff
                } else {
                    r - g + 4 * diff
                };
                let h = (h * hdiv[diff as usize] + 2048) >> 12;
                [(if h < 0 { h + 180 } else { h }) as u8, s as u8, v as u8]
            })
            .collect()
    }
    pub fn from_rgb(width: u32, height: u32, mut pixels: Vec<u8>) -> Result<Self, String> {
        for p in pixels.chunks_exact_mut(3) {
            p.swap(0, 2);
        }
        Self::new(width, height, pixels)
    }
    pub fn resize(
        &self,
        width: u32,
        height: u32,
        interpolation: Interpolation,
    ) -> Result<Self, String> {
        if width == 0 || height == 0 {
            return Err("resize dimensions must be positive".into());
        }
        if (width, height) == (self.width, self.height) {
            return Ok(self.clone());
        }
        match interpolation {
            Interpolation::Area if width <= self.width && height <= self.height => {
                self.resize_area(width, height)
            }
            Interpolation::Area => Err("area enlargement is not used by OCR; use linear".into()),
            Interpolation::Linear => self.resize_linear(width, height),
        }
    }
    fn resize_area(&self, width: u32, height: u32) -> Result<Self, String> {
        let xtab = area_weights(self.width, width);
        let ytab = area_weights(self.height, height);
        let mut pixels = vec![0; width as usize * height as usize * 3];
        // Integer-factor INTER_AREA uses an integer sum and one f32 multiply.
        let integer = self.width % width == 0 && self.height % height == 0;
        for y in 0..height as usize {
            for x in 0..width as usize {
                for c in 0..3 {
                    let mut sum = 0.0f32;
                    if integer {
                        let mut total = 0u32;
                        for &(sy, _) in &ytab[y] {
                            for &(sx, _) in &xtab[x] {
                                total +=
                                    self.pixels[(sy * self.width as usize + sx) * 3 + c] as u32;
                            }
                        }
                        let count = (self.width / width) * (self.height / height);
                        sum = total as f32 * (1.0 / count as f32);
                        // OpenCV's specialized 2x2 uint8 path rounds half up.
                        if count == 4 && self.width / width == 2 {
                            sum = (total + 2) as f32 / 4.0;
                            sum = sum.floor();
                        }
                    } else {
                        for &(sy, beta) in &ytab[y] {
                            let mut horizontal = 0.0f32;
                            for &(sx, alpha) in &xtab[x] {
                                horizontal += self.pixels[(sy * self.width as usize + sx) * 3 + c]
                                    as f32
                                    * alpha;
                            }
                            sum += horizontal * beta;
                        }
                    }
                    pixels[(y * width as usize + x) * 3 + c] =
                        sum.round_ties_even().clamp(0., 255.) as u8;
                }
            }
        }
        Self::new(width, height, pixels)
    }
    fn resize_linear(&self, width: u32, height: u32) -> Result<Self, String> {
        let mut pixels = vec![0; width as usize * height as usize * 3];
        let coordinates = |d: u32, source: u32, dest: u32, horizontal: bool| {
            let f = ((d as f64 + 0.5) * (source as f64 / dest as f64) - 0.5) as f32;
            let mut s = f.floor() as i32;
            let mut f = f - s as f32;
            if horizontal && (s < 0 || s >= source as i32 - 1) {
                s = s.clamp(0, source as i32 - 1);
                f = 0.;
            }
            (
                s,
                ((1. - f) * 2048.).round_ties_even() as i32,
                (f * 2048.).round_ties_even() as i32,
            )
        };
        for y in 0..height {
            let (sy, b0, b1) = coordinates(y, self.height, height, false);
            let y0 = sy.clamp(0, self.height as i32 - 1) as usize;
            let y1 = (sy + 1).clamp(0, self.height as i32 - 1) as usize;
            for x in 0..width {
                let (sx, a0, a1) = coordinates(x, self.width, width, true);
                let x0 = sx as usize;
                let x1 = (sx + 1).min(self.width as i32 - 1) as usize;
                for c in 0..3 {
                    let h0 = self.pixels[(y0 * self.width as usize + x0) * 3 + c] as i32 * a0
                        + self.pixels[(y0 * self.width as usize + x1) * 3 + c] as i32 * a1;
                    let h1 = self.pixels[(y1 * self.width as usize + x0) * 3 + c] as i32 * a0
                        + self.pixels[(y1 * self.width as usize + x1) * 3 + c] as i32 * a1;
                    pixels[(y as usize * width as usize + x as usize) * 3 + c] =
                        ((((b0 * (h0 >> 4)) >> 16) + ((b1 * (h1 >> 4)) >> 16) + 2) >> 2) as u8;
                }
            }
        }
        Self::new(width, height, pixels)
    }
}

#[derive(Clone, Copy, Debug)]
pub enum Interpolation {
    Area,
    Linear,
}

fn area_weights(source: u32, dest: u32) -> Vec<Vec<(usize, f32)>> {
    let scale = source as f64 / dest as f64;
    (0..dest)
        .map(|d| {
            let start = d as f64 * scale;
            let end = start + scale;
            let cell = scale.min(source as f64 - start);
            let s2 = (end.floor() as u32).min(source - 1);
            let s1 = (start.ceil() as u32).min(s2);
            let mut row = Vec::new();
            if s1 as f64 - start > 1e-3 {
                row.push(((s1 - 1) as usize, ((s1 as f64 - start) / cell) as f32));
            }
            for s in s1..s2 {
                row.push((s as usize, (1. / cell) as f32));
            }
            if end - s2 as f64 > 1e-3 {
                row.push((
                    s2 as usize,
                    ((end - s2 as f64).min(1.).min(cell) / cell) as f32,
                ));
            }
            row
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateMode {
    None,
    Light,
    Full,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GateResult {
    pub passed: bool,
    pub reason: &'static str,
}
impl GateResult {
    fn new(passed: bool, reason: &'static str) -> Self {
        Self { passed, reason }
    }
}

pub fn brightness_gate(
    frame: &BgrFrame,
    roi: [f64; 4],
    width: u32,
    mode: GateMode,
) -> Result<GateResult, String> {
    if mode == GateMode::None {
        return Ok(GateResult::new(true, "disabled"));
    }
    let Some(crop) = crop(frame, roi)? else {
        return Ok(GateResult::new(true, "empty-roi"));
    };
    let width = width.max(1);
    // Python round is ties-to-even here, unlike OCR crop's truncation.
    let height = (crop.height as f64 * (width as f64 / crop.width as f64))
        .round_ties_even()
        .max(1.) as u32;
    let interpolation = if width < crop.width {
        Interpolation::Area
    } else {
        Interpolation::Linear
    };
    let crop = crop.resize(width, height, interpolation)?;
    let gray = crop.to_gray();
    if mode == GateMode::Light {
        let bright = gray.iter().filter(|&&v| v >= 155).count();
        let threshold = 8usize.max((width as f64 * height as f64 * 0.002) as usize);
        return Ok(if bright >= threshold {
            GateResult::new(true, "light-brightness-passed")
        } else {
            GateResult::new(false, "light-low-brightness")
        });
    }
    let (w, h) = (width as usize, height as usize);
    let hsv = crop.to_hsv();
    let dark: Vec<u8> = gray.iter().map(|&v| u8::from(v <= 100)).collect();
    let dark_near = morph(&dark, w, h, 2, 2, true);
    let outlined: Vec<u8> = hsv
        .chunks_exact(3)
        .zip(dark_near)
        .map(|(p, d)| {
            u8::from(
                d != 0
                    && ((p[1] <= 105 && p[2] >= 155)
                        || (p[0] >= 3 && p[0] <= 42 && p[1] >= 70 && p[2] >= 135)),
            )
        })
        .collect();
    // boxFilter defaults to BORDER_REFLECT_101, unlike morphology's constant border.
    let mut grouped = vec![0u8; w * h];
    let mut rows = vec![0usize; h];
    let mut count = 0usize;
    for y in 0..h {
        for x in 0..w {
            if outlined[y * w + x] == 0 {
                continue;
            }
            let mut neighbours = 0;
            for dy in -2..=2 {
                for dx in -3..=3 {
                    neighbours += outlined
                        [reflect101(y as isize + dy, h) * w + reflect101(x as isize + dx, w)]
                        as usize;
                }
            }
            if neighbours >= 3 {
                grouped[y * w + x] = 1;
                rows[y] += 1;
                count += 1;
            }
        }
    }
    if count < 1450 {
        return Ok(GateResult::new(false, "insufficient-bright-outline"));
    }
    let max_rows = (0..h)
        .map(|y| {
            rows[y.saturating_sub(5)..(y + 6).min(h)]
                .iter()
                .sum::<usize>()
        })
        .max()
        .unwrap_or(0);
    if max_rows < 1000 {
        return Ok(GateResult::new(false, "insufficient-horizontal-structure"));
    }
    let dilated = morph(&grouped, w, h, 5, 1, true);
    let mut closed = morph(&dilated, w, h, 5, 1, false);
    let max_height = 3usize.max((h as f64 * 0.6) as usize);
    for y in 0..h {
        for x in 0..w {
            if closed[y * w + x] == 0 {
                continue;
            }
            closed[y * w + x] = 0;
            let mut stack = vec![(x, y)];
            let (mut left, mut right, mut top, mut bottom) = (x, x, y, y);
            while let Some((x, y)) = stack.pop() {
                left = left.min(x);
                right = right.max(x);
                top = top.min(y);
                bottom = bottom.max(y);
                for ny in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                    for nx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                        if closed[ny * w + nx] != 0 {
                            closed[ny * w + nx] = 0;
                            stack.push((nx, ny));
                        }
                    }
                }
            }
            let height = bottom - top + 1;
            if height >= 3 && height <= max_height && right - left + 1 >= 20 {
                return Ok(GateResult::new(true, "passed"));
            }
        }
    }
    Ok(GateResult::new(false, "no-connected-text"))
}
fn reflect101(mut p: isize, len: usize) -> usize {
    if len == 1 {
        return 0;
    }
    while p < 0 || p >= len as isize {
        p = if p < 0 { -p } else { 2 * len as isize - p - 2 };
    }
    p as usize
}
fn morph(src: &[u8], w: usize, h: usize, rx: usize, ry: usize, dilate: bool) -> Vec<u8> {
    // Separable rectangular kernel. Outside pixels are neutral (OpenCV morphologyDefaultBorderValue).
    let mut horizontal = vec![0; w * h];
    let mut dst = vec![0; w * h];
    for y in 0..h {
        for x in 0..w {
            let window = &src[y * w + x.saturating_sub(rx)..y * w + (x + rx + 1).min(w)];
            horizontal[y * w + x] = if dilate {
                *window.iter().max().unwrap()
            } else {
                *window.iter().min().unwrap()
            };
        }
    }
    for y in 0..h {
        for x in 0..w {
            let window =
                (y.saturating_sub(ry)..(y + ry + 1).min(h)).map(|yy| horizontal[yy * w + x]);
            dst[y * w + x] = if dilate {
                window.max().unwrap()
            } else {
                window.min().unwrap()
            };
        }
    }
    dst
}

pub fn validate_roi(roi: [f64; 4]) -> Result<(), String> {
    let [x1, y1, x2, y2] = roi;
    if roi.iter().all(|n| n.is_finite())
        && 0. <= x1
        && x1 < x2
        && x2 <= 1.
        && 0. <= y1
        && y1 < y2
        && y2 <= 1.
    {
        Ok(())
    } else {
        Err("ROI values must be finite ascending ratios between 0 and 1".into())
    }
}
fn crop(frame: &BgrFrame, roi: [f64; 4]) -> Result<Option<BgrFrame>, String> {
    validate_roi(roi)?;
    let [x1, y1, x2, y2] = roi;
    let x1 = (frame.width as f64 * x1) as u32;
    let x2 = (frame.width as f64 * x2) as u32;
    let y1 = (frame.height as f64 * y1) as u32;
    let y2 = (frame.height as f64 * y2) as u32;
    if x1 == x2 || y1 == y2 {
        return Ok(None);
    }
    let mut pixels = Vec::with_capacity(((x2 - x1) * (y2 - y1) * 3) as usize);
    for y in y1..y2 {
        let offset = (y * frame.width + x1) as usize * 3;
        pixels.extend_from_slice(&frame.pixels[offset..offset + (x2 - x1) as usize * 3]);
    }
    Ok(Some(BgrFrame::new(x2 - x1, y2 - y1, pixels)?))
}
pub fn crop_for_ocr(frame: &BgrFrame, roi: [f64; 4], ocr_width: u32) -> Result<BgrFrame, String> {
    let crop = crop(frame, roi)?.ok_or("empty OCR ROI")?;
    if ocr_width > 0 && crop.width > ocr_width {
        let height = (crop.height as f64 * (ocr_width as f64 / crop.width as f64)) as u32;
        crop.resize(ocr_width, height.max(1), Interpolation::Area)
    } else {
        Ok(crop)
    }
}
