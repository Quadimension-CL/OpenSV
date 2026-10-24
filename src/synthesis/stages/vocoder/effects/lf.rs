use super::{math,warp};

struct Pulse {t:f32,te:f32,ta:f32,wp:f32,sin:f32,cos:f32,eps:f32,alpha:f32,e0:f32,scale:f32}
impl Pulse {
    fn new(rd:f32,hz:f32)->Self {
        let original=1.0/hz;
        let t=original.max(0.00125);
        let scale=if original<0.00125 {(0.00125/f64::from(original)) as f32}else{1.0};
        let (ra,rk,rk0)=if rd<2.7 {
            let ra=if rd<0.21 {1e-6}else{(f64::from(rd)*0.048-0.01) as f32};
            let rk0=(f64::from(rd)*0.118+0.224) as f32;
            let rk=(f64::from(rk0*0.25)/(f64::from(rd)*0.11/(f64::from(rk0)*1.2+0.5)-f64::from(ra))) as f32;
            (ra,rk,rk0)
        }else{
            let preliminary=(1.0-0.4608294930875576/f64::from(rd)) as f32;
            let rk=(5.96/(7.96-f64::from(preliminary+preliminary))+0.0093552) as f32;
            ((0.323/f64::from(rd)) as f32,rk,(f64::from((rk+rk)*preliminary)-1.0428) as f32)
        };
        let tp=(0.5/rk)*t;
        let te=((rk0+1.0)*(0.5/rk))*t;
        let ta=ra*t;
        let wp=(std::f64::consts::PI/f64::from(tp)) as f32;
        let (sin,cos)=(te*wp).sin_cos();
        let eps=root(|x|((1.0-f64::from((te-t)*x).exp())-f64::from(ta*x)) as f32,
            1.0,(2.0/(f64::from(ta)+1e-9)) as f32);
        let gap=te-t;let e=(gap*eps).exp();
        let area=(1.0-e)/((eps*eps)*ta)+(e*gap)/(ta*eps);
        let constant=((wp*wp)*sin)*area-cos*wp;
        let double=(sin*2.0)*area;
        let single=area*sin;
        let tewp=te*wp;
        let mut alpha=0.0_f32;
        for _ in 0..8 {
            let e=f64::from(-(te*alpha)).exp();
            let numerator=(f64::from((single*alpha+sin)*alpha)+f64::from(wp)*e+f64::from(constant)) as f32;
            let denominator=(f64::from(double*alpha+sin)-f64::from(tewp)*e) as f32;
            alpha-=numerator/denominator;
        }
        let e0=(-1.0/(f64::from(te*alpha).exp()*f64::from(sin))) as f32;
        Self{t,te,ta,wp,sin,cos,eps,alpha,e0,scale}
    }
    fn magnitude(&self,frequency:f32)->f32 {
        let w=(f64::from(frequency)*std::f64::consts::TAU/f64::from(self.scale)) as f32;
        let c=self.ta*self.eps;
        let ret=(1.0-c)*self.eps;
        let real=(self.alpha*self.alpha-w*w)+self.wp*self.wp;
        let imaginary=(-w*self.alpha)*2.0;
        let inverse=1.0/(real*real+imaginary*imaginary);
        let sr=(real*self.e0)*inverse;
        let si=-((imaginary*self.e0)*inverse);
        let angle=-w*self.te;
        let amplitude=math::exp(self.te*self.alpha);
        let er=math::sin(angle+1.570_796_4)*amplitude;
        let ei=math::sin(angle)*amplitude;
        let br=self.alpha*self.sin-self.wp*self.cos;
        let bi=-w*self.sin;
        let pr=(er*br-ei*bi)+self.wp;
        let pi=ei*br+er*bi;
        let ore=sr*pr-si*pi;
        let oim=si*pr+sr*pi;
        let rr=-((c*w)*w);
        let ri=(c*w)*self.eps;
        let inverse=1.0/(rr*rr+ri*ri);
        let er=math::sin(angle+1.570_796_4)*math::exp(0.0);
        let ei=math::sin(angle)*math::exp(0.0);
        let rs=(er*rr+ei*ri)*inverse;
        let is=(ei*rr-er*ri)*inverse;
        let (br,bi)=if ret<1.0 {(0.0,-(c*w))}else{
            let angle=(self.te-self.t)*w;
            ((1.0-math::sin(angle+1.570_796_4)*math::exp(0.0))*ret,
                -(math::sin(angle)*math::exp(0.0)*ret)-(c*w))
        };
        let re=ore+(br*rs-bi*is);
        let im=oim+(bi*rs+br*is);
        let result=(re*re+im*im).sqrt();
        if result.is_nan(){0.0}else{result}
    }
}

fn root(function:impl Fn(f32)->f32,mut a:f32,mut b:f32)->f32 {
    let mut fa=function(a);let mut fb=function(b);
    if fa*fb>=0.0 {return (a+b)*0.5;}
    if fa.abs()<fb.abs(){std::mem::swap(&mut a,&mut b);std::mem::swap(&mut fa,&mut fb);}
    let mut c=a;let mut fc=fa;let mut d=0.0;let mut flag=true;
    for _ in 0..200 {
        if fb.abs()<=1e-8 || (a-b).abs()<=1e-8 {break;}
        let mut s=if fa!=fc && fb!=fc {
            ((b*fa)*fc)/((fb-fc)*(fb-fa))+((a*fb)*fc)/((fa-fc)*(fa-fb))
                +((c*fa)*fb)/((fc-fb)*(fc-fa))
        }else {b-((b-a)*fb)/(fb-fa)};
        let mut bad=f64::from(s)<f64::from(b)*0.25+f64::from(a)*0.75 || s>b;
        if !bad {
            let limit=if flag {(b-c).abs()}else{(c-d).abs()};
            bad=f64::from((s-b).abs())>=f64::from(limit)*0.5 || limit<1e-8;
        }
        if bad {s=(a+b)*0.5;flag=true;if s==a || s==b {break;}}
        else {flag=false;}
        let fs=function(s);d=c;c=b;fc=fb;
        if fs*fa<0.0 {b=s;fb=fs;}else{a=s;fa=fs;}
        if fa.abs()<fb.abs(){std::mem::swap(&mut a,&mut b);std::mem::swap(&mut fa,&mut fb);}
    }
    b
}

pub(super) fn correction(old:f32,new:f32,hz:f32,bins:usize)->Vec<f32> {
    let count=(24000.0/hz) as usize;
    if count<2 {return vec![0.0;bins];}
    let old=Pulse::new(old,hz);let new=Pulse::new(new,hz);
    let last=hz*count as f32;
    let frequencies=(0..count).map(|i|i as f32*(last-hz)/(count-1) as f32+hz);
    let mut before=Vec::with_capacity(count);let mut after=Vec::with_capacity(count);
    for f in frequencies {before.push(old.magnitude(f));after.push(new.magnitude(f));}
    let baseline=[before[0],after[0]];
    for i in 1..count {before[i]=(before[i]/baseline[0])/(i+1) as f32;after[i]=(after[i]/baseline[1])/(i+1) as f32;}
    before[0]=1.0;after[0]=1.0;
    let ratios=after.iter().zip(before).map(|(&a,b)|math::log(a/b)).collect::<Vec<_>>();

    warp::uniform(&ratios,count as f32,last,(0..bins).map(|i|i as f32*24000.0/bins as f32))
}
