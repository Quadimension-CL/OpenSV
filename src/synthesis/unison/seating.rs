use super::{SeatingPlan, UnisonSettings};
use crate::synthesis::voice::{Result, Speaker, VoiceError};

const ROWS: [[usize;3];17]=[
    [0,0,0],[1,0,0],[2,0,0],[3,0,0],[4,0,0],[5,0,0],[6,0,0],[7,0,0],
    [8,0,0],[4,5,0],[5,5,0],[5,6,0],[6,6,0],[6,7,0],[6,8,0],[4,5,6],[5,6,5],
];

pub(super) fn prepare(settings:&UnisonSettings,speakers:&[Speaker])->Result<SeatingPlan> {
    let cropped=settings.full_size>=settings.count && settings.full_size>0 && settings.center>=0;
    let size=if cropped {settings.full_size}else{settings.count};
    if size==0||size>4096 {return Err(VoiceError::new("齐唱成员数量无效"))}

    let part=speakers.iter().find(|s|s.part==settings.part).or_else(||speakers.first())
        .map_or_else(String::new,|s|s.part.clone());
    let pool:Vec<_>=speakers.iter().filter(|s|s.part==part).map(|s|Some(s.id)).collect();
    let pool=if pool.is_empty(){vec![None]}else{pool};
    let mut rng=Random::new(settings.seed);
    let selected=if size==1 {
        vec![pool[settings.seed as usize%pool.len()]]
    } else if size<=pool.len() {
        let mut remaining=pool;
        (0..size).map(|_| {let index=rng.index(remaining.len());remaining.remove(index)}).collect()
    } else {
        (0..size).map(|_|pool[rng.index(pool.len())]).collect()
    };
    let first=if cropped {(i64::from(settings.center)-(settings.count as i64+1)/2)
        .clamp(0,(size-settings.count) as i64) as usize}else{0};
    Ok(SeatingPlan {part,rows:rows(size),speakers:selected,first,count:settings.count})
}

fn rows(count:usize)->Vec<Vec<usize>> {
    let counts:Vec<usize>=if count<=16 {ROWS[count].into_iter().filter(|&n|n>0).collect()}
        else{vec![count]};
    if counts.len()==1 {return vec![(0..count).collect()]}
    let mut seats=Vec::with_capacity(count);
    for (row,&length) in counts.iter().enumerate() {
        for column in 0..length {

            seats.push((2*column as i32-(length as i32-1),row,column));
        }
    }
    seats.sort_by_key(|&(x,row,_)|(x,row));
    let mut first=0;let mut reverse=false;
    while first<seats.len() {
        let end=first+seats[first..].iter().take_while(|s|s.0==seats[first].0).count();
        if reverse {seats[first..end].reverse()}
        reverse=!reverse;first=end;
    }
    let mut result:Vec<_>=counts.iter().map(|&n|vec![0;n]).collect();
    for (index,(_,row,column)) in seats.into_iter().enumerate() {result[row][column]=index}
    result
}

struct Random(u64);
impl Random {

    fn new(seed:u32)->Self {Self((u64::from(seed).wrapping_mul(0x121f9f058e89)
        .wrapping_add(0x7455bfb52a42))&0xffffffffffff)}
    fn index(&mut self,length:usize)->usize {
        self.0=(self.0.wrapping_mul(0x5deece66d).wrapping_add(11))&0xffffffffffff;
        (((self.0>>16)*length as u64)>>32) as usize
    }
}
