fn pieces(template:&str)->Option<(Vec<String>,Vec<String>)> {
    let mut literals=vec![String::new()];let mut names=Vec::new();let mut input=template.chars().peekable();
    while let Some(c)=input.next() {
        if c=='{' && input.peek()==Some(&'{') {input.next();literals.last_mut()?.push('{');}
        else if c=='}' && input.peek()==Some(&'}') {input.next();literals.last_mut()?.push('}');}
        else if c=='{' {
            let mut slot=String::new();loop {let c=input.next()?;if c=='}' {break;}slot.push(c);}
            names.push(slot.split(':').next()?.into());literals.push(String::new());
        }else {literals.last_mut()?.push(c);}
    }
    Some((literals,names))
}
pub(super) fn translate(source:&str,target:&str,rendered:&str)->Option<String> {
    let (parts,names)=pieces(source)?;
    if names.is_empty() {return (rendered==parts[0]).then(||target.into());}
    let mut rest=rendered.strip_prefix(&parts[0])?;let mut values=Vec::new();
    for (index,tail) in parts.iter().skip(1).enumerate() {
        let end=if index+1==names.len() {
            rest.len().checked_sub(tail.len()).filter(|end|rest.get(*end..)==Some(tail.as_str()))?
        }else if tail.is_empty() {0}else{rest.find(tail)?};
        values.push(rest.get(..end)?);rest=rest.get(end+tail.len()..)?;
    }
    if !rest.is_empty() {return None;}
    let (translated,slots)=pieces(target)?;if slots.len()!=values.len() {return None;}
    let mut result=translated[0].clone();
    for (index,name) in slots.iter().enumerate() {
        let source_index=if name.is_empty(){index}else{name.parse::<usize>().ok().or_else(||names.iter().position(|s|s==name))?};
        let value=values.get(source_index)?;
        if matches!(names.get(source_index).map(String::as_str),Some("error"|"e"|"reason")) && value.len()<rendered.len() {
            result.push_str(&super::message(value));
        }else {result.push_str(value);}
        result.push_str(&translated[index+1]);
    }
    Some(result)
}
