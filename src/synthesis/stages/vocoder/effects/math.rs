pub(super) fn log(value:f32)->f32 {
    let bits=value.to_bits()&0x7fff_ffff;
    let mantissa=f32::from_bits((bits&0x7f_ffff)|0x3f00_0000);
    let exponent=bits as f32*1.192_092_9e-7_f32-124.225_52_f32;
    let numerator=-1.498_030_3_f32*mantissa+exponent;
    (numerator+(-1.725_88_f32)/(mantissa+0.352_088_72_f32))*0.693_147_2_f32
}
pub(super) fn exp(value:f32)->f32 {
    pow2(value*1.442_695_f32)
}
pub(super) fn pow2(power:f32)->f32 {
    let clipped=power.max(-126.0);
    let fraction=clipped-clipped.trunc()+if power<0.0 {1.0}else{0.0};
    let quotient=27.728_024_f32/(4.842_525_5_f32-fraction);
    let offset=quotient+(clipped+121.274_055_f32);
    let bits=(-1.490_129_1_f32*fraction+offset)*8_388_608.0;
    f32::from_bits(bits as u32)
}
pub(super) fn sin(value:f32)->f32 {
    let cycles=(value*0.159_154_94_f32).trunc()+if value<0.0 {-0.5}else{0.5};
    let reduced=cycles*6.283_185_5_f32-value;
    let y=reduced*1.273_239_5_f32-(reduced*0.405_284_73_f32)*reduced.abs();
    let square=y*y;
    let sign=reduced.to_bits()&0x8000_0000;
    let a=f32::from_bits(sign^0xbb53_3217);
    let b=f32::from_bits(sign|0.015_124_941_f32.to_bits());
    let c=f32::from_bits(sign|0.203_639_37_f32.to_bits());
    y*0.784_444_87_f32+((a*square+b)*square+c)*square
}
