use crate::model::Language;

pub(in crate::synthesis::score) fn split(phones:Vec<String>,nuclei:&[usize],language:Language)->Vec<Vec<String>> {
    let mut parts=Vec::new();let mut start=0;
    for pair in nuclei.windows(2) {
        let first=pair[0]+1;let next=pair[1];
        let boundary=if language==Language::German {

            (first..next).find(|&i|phones[i]=="?")
                .unwrap_or_else(||(first..=next).find(|&i|valid(&phones[i..next],language)).unwrap_or(next))
        }else{(first..=next).find(|&i|valid(&phones[i..next],language)).unwrap_or(next)};
        parts.push(phones[start..boundary].to_vec());start=boundary;
    }
    if start<phones.len(){parts.push(phones[start..].to_vec())}
    parts
}

fn valid(phones:&[String],language:Language)->bool {
    if phones.is_empty(){return true}
    if language==Language::German && phones[0]=="N" {return false}
    if phones.len()==1{return true}
    if language==Language::French && matches!(phones.last().map(String::as_str),Some("j"|"w"|"H")) {
        return valid(&phones[..phones.len()-1],language)
    }
    let cluster=phones.join(" ");
    match language {
        Language::German=>matches!(cluster.as_str(),
            "p f"|"t s"|"t S"|"p l"|"b l"|"k l"|"g l"|"f l"|"S l"|
            "p R"|"b R"|"t R"|"d R"|"k R"|"g R"|"f R"|"S R"|"S p"|"S t"|"s k"|"k v"|
            "S p R"|"S p l"|"S t R"),
        Language::French=>matches!(cluster.as_str(),
            "p l"|"b l"|"k l"|"g l"|"f l"|"p R"|"b R"|"t R"|"d R"|"k R"|"g R"|"f R"|"v R"|
            "s p"|"s t"|"s k"|"s f"|"s m"|"s n"|"s l"|"s p R"|"s t R"|"s k R"|"s p l"),
        Language::Portuguese=>matches!(cluster.as_str(),
            "p l"|"b l"|"k l"|"g l"|"f l"|"p 4"|"b 4"|"t 4"|"d 4"|"k 4"|"g 4"|"f 4"|"v 4"),
        _=>false,
    }
}
