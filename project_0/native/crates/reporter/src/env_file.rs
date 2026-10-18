use std::path::{Path,PathBuf};
#[derive(Debug,Default)]
pub struct Loaded { pub file: PathBuf, pub set: Vec<String>, pub kept_existing: Vec<String> }
pub fn parse(input:&str)->Result<Vec<(String,String)>,String>{
 let mut out=Vec::new();
 for (i,line) in input.lines().enumerate(){
  let line=line.trim();
  if line.is_empty()||line.starts_with('#'){continue}
  let line=line.strip_prefix("export ").unwrap_or(line).trim_start();
  let (key,value)=line.split_once('=').ok_or_else(||format!("line {}: expected KEY=VALUE",i+1))?;
  let key=key.trim();
  let mut chars=key.chars();
  if !matches!(chars.next(),Some(c) if c.is_ascii_uppercase()||c=='_')||!chars.all(|c|c.is_ascii_uppercase()||c.is_ascii_digit()||c=='_'){
   return Err(format!("line {}: invalid key",i+1));
  }
  let value=value.trim();
  let value=if value.len()>=2 && ((value.starts_with('"')&&value.ends_with('"'))||(value.starts_with("'")&&value.ends_with("'"))){&value[1..value.len()-1]}else{value};
  out.push((key.to_owned(),value.to_owned()));
 }
 Ok(out)
}
pub fn load(path:&Path)->Result<Loaded,String>{
 let text=std::fs::read_to_string(path).map_err(|e|format!("env file read {}: {e}",path.display()))?;
 let entries=parse(&text).map_err(|e|format!("{}: {e}",path.display()))?;
 let mut loaded=Loaded{file:path.to_path_buf(),..Default::default()};
 for (key,value) in entries{
  if std::env::var_os(&key).is_some(){loaded.kept_existing.push(key);}
  else{std::env::set_var(&key,value);loaded.set.push(key);}
 }
 Ok(loaded)
}
pub fn load_list(spec:&str)->Result<Vec<Loaded>,String>{
 spec.split(':').map(str::trim).filter(|v|!v.is_empty()).map(|p|load(Path::new(p))).collect()
}
#[cfg(test)]
mod tests{
 use super::*;
 #[test]fn parse_basic(){assert_eq!(parse("export A=1\nB=\"two words\"\n").unwrap(),vec![("A".into(),"1".into()),("B".into(),"two words".into())]);}
 #[test]fn no_secret_in_errors(){let e=parse("SENSITIVE_VALUE").unwrap_err();assert!(!e.contains("SENSITIVE_VALUE"));assert!(parse("bad-key=123").is_err());}
}
