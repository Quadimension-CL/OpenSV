use std::collections::HashMap;

pub(super) fn link(lyrics:&[&str],words:&mut [Vec<Vec<String>>],classes:&HashMap<String,String>) {
    let vowel=|phone:&str|classes.get(phone).is_some_and(|class|matches!(class.as_str(),"vowel"|"diphthong"));
    for index in 0..words.len().saturating_sub(1) {
        let (left,right)=words.split_at_mut(index+1);
        let current=&mut left[index];let next=&mut right[0];
        if !next.first().and_then(|s|s.first()).is_some_and(|p|vowel(p)) {continue}
        let Some(last)=current.last_mut() else {continue};
        if last.is_empty(){continue}
        let spelling=lyrics[index].split_whitespace().next_back().unwrap_or("").to_lowercase();
        let key=spelling.split_once('\'').map_or(spelling.as_str(),|(_,tail)|tail);
        if WORDS.contains(&key) && let Some(phone)=liaison(key) {last.push(phone.into())}
        if last.last().is_some_and(|p|!vowel(p)) {
            let phone=last.pop().expect("nonempty last syllable");
            next[0].insert(0,phone);
        }
    }
}

fn liaison(word:&str)->Option<&'static str> {
    match word.as_bytes().last()? {
        b's'|b'x'=>Some("z"),b't'|b'd'=>Some("t"),b'n'=>Some("n"),
        b'p'=>Some("p"),b'g'=>Some("g"),_=>None,
    }
}

const WORDS:&[&str]=&[
    "ancien","ancienne","anciennes","anciens","aucun","aucune","aucunes","aucuns","aux",
    "avaient","avait","bas","basse","basses","beau","beaux","belle","belles","bien","bon",
    "bonne","bonnes","bons","cent","cents","certaines","certains","ces","ceux","cheztrès",
    "court","courte","courtes","courts","dans","des","deux","dix","elles","en","est","eux",
    "fait","gentil","gentille","gentilles","gentils","grand","grande","grandes","grands",
    "gros","grosse","grosses","haut","haute","hautes","hauts","ils","jeune","jeunes",
    "joli","jolie","jolies","jolis","les","long","longs","longue","longues","mauvais",
    "mauvaise","mauvaises","mes","mieux","moins","mon","nous","nouveau","nouveaux",
    "nouvelle","nouvelles","on","ont","petit","petite","petites","petits","peux","plus",
    "plusieurs","quand","quelles","quelques","quels","sans","ses","six","son","sont",
    "sous","tes","ton","tous","tout","toutes","trois","un","veux","vieille","vieilles",
    "vieux","vous","étaient","était",
];
