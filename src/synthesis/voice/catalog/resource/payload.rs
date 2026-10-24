use std::{fs::File,io::{Read,Seek,SeekFrom}};
use xxhash_rust::xxh3::Xxh3;
use crate::synthesis::voice::{Result,VoiceError,array,u32_at,stream,format::FormatContext};

pub(super) fn read(file:&mut File,record:&[u8],material:&[u8;32],context:&FormatContext,limit:usize) -> Result<Vec<u8>> {
    let length=u32_at(record,52)? as usize;
    if length>limit {return Err(VoiceError::new("声库元数据资源超过读取限制"));}
    file.seek(SeekFrom::Start(u32_at(record,48)? as u64))?;
    let mut source=vec![0u8;length];file.read_exact(&mut source)?;
    let nonce:[u8;12]=array(record,36)?;
    let flag=u32_at(record,32)?;
    let mut digest=Xxh3::new();
    digest.update(&((length+20) as u32).to_le_bytes());digest.update(&nonce);
    digest.update(material);digest.update(&record[16..32]);digest.update(&flag.to_le_bytes());
    digest.update(&source);digest.update(material);digest.update(&nonce);
    if digest.digest128().to_le_bytes()!=record[..16] {
        return Err(VoiceError::new("声库元数据资源校验失败"));
    }
    if flag==1 {source=stream::decode(&source,&context.constants,material,&nonce,0)?;}
    Ok(source)
}
