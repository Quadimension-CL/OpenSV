use super::Automation;

impl Automation {
    pub fn sample(&self, position: i64, default: f32) -> f32 {
        let n = self.points.len();
        if n == 0 { return default; }
        let right = self.points.partition_point(|p| p.0 < position);
        if right == 0 { return self.points[0].1 as f32; }
        if right == n { return self.points[n-1].1 as f32; }
        let (x0, y0) = self.points[right-1];
        let (x1, y1) = self.points[right];
        let (y0, y1) = (y0 as f32, y1 as f32);
        if position == x1 { return y1; }
        let span = (i128::from(x1)-i128::from(x0)) as f32;
        let t = (i128::from(position)-i128::from(x0)) as f32 / span;
        if self.mode != "cubic" || n <= 2 {
            let t = if self.mode == "cosine" {
                ((1.0 - (f64::from(t)*3.1415926535).cos()) * 0.5) as f32
            } else { t };
            return (1.0-t)*y0 + t*y1;
        }
        let before = if right >= 2 { self.points[right-2] } else { (x0.saturating_sub(1), f64::from(y0)) };
        let after = self.points.get(right+1).copied().unwrap_or((x1.saturating_add(1), f64::from(y1)));
        let m0 = ((y1-before.1 as f32) / (i128::from(x1)-i128::from(before.0)) as f32)*span;
        let m1 = ((after.1 as f32-y0) / (i128::from(after.0)-i128::from(x0)) as f32)*span;
        let damp = (span/705600000.0_f32-0.25).max(0.0);
        let alpha = if damp > 4.0 { 0.0 } else { (1.0/(1.0+f64::from(damp*10.0))) as f32 };
        let linear = (1.0-f64::from(alpha))*f64::from((y1-y0)/span);
        let m0 = f64::from((f64::from(alpha*m0)+linear) as f32);
        let m1 = f64::from((f64::from(alpha*m1)+linear) as f32);
        let t2 = t*t;
        let t3 = t2*t;
        let (t,t2,t3) = (f64::from(t),f64::from(t2),f64::from(t3));
        (((t3-2.0*t2+t)*m0+(2.0*t3-3.0*t2+1.0)*f64::from(y0))
            +(3.0*t2-2.0*t3)*f64::from(y1)+(t3-t2)*m1) as f32
    }
}
