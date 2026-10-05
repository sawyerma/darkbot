"""Compile the exact checked-in admission predicate with adversarial traces.
PASS means the counterexample was reproduced, not that admission is safe.
Usage: python3 cache_counterexamples.py REPOSITORY_ROOT
"""
from pathlib import Path
import subprocess,sys,tempfile
root=Path(sys.argv[1]); p=root/'project_0/native/crates/darkbot-runner/src/main.rs'
s=p.read_text(); a=s.index('enum CandidateHealthState'); b=s.index('// [FIX-121]',a)
code=s[a:b].replace('#[cfg(feature = "crate_health")]','')
code='#[derive(Clone, Copy, Debug, PartialEq, Eq)]\n'+code
code+='''
fn main() {
 use CandidateHealthState::*;
 let e=|state,last_forwarded_slot| CandidateCacheEntry{state,last_forwarded_slot};
 assert!(candidate_should_forward(Some(e(Healthy,100)),Liquidatable,100,1,false,false));
 assert!(!candidate_should_forward(Some(e(Liquidatable,100)),Healthy,100,1,true,true));
 assert!(candidate_should_forward(Some(e(Healthy,100)),Liquidatable,100,1,false,false));
 assert!(candidate_should_forward(Some(e(Error,100)),Liquidatable,100,1,false,false));
 assert!(candidate_should_forward(Some(e(Liquidatable,100)),Liquidatable,100,1,true,false));
 println!("PASS observed class transitions and forced Bank rearm");
 assert!(!candidate_should_forward(Some(e(Liquidatable,100)),Liquidatable,100,1,false,true));
 println!("REPRODUCED: second same-slot price/profit change suppressed despite free capacity");
 assert!(!candidate_should_forward(Some(e(Error,100)),Error,100,1,false,true));
 println!("REPRODUCED: same Error class after changed integration input suppressed");
 // Main loop sorts by pubkey, not by last_forwarded_slot. Model 1,025 persistent
 // candidates with all outstanding work drained between one dirty batch per slot.
 let mut entries=vec![e(Liquidatable,100);1025];
 let mut forwards=vec![0u64;1025];
 for slot in 101..=200 {
   let mut pending=0;
   for i in 0..entries.len() {
     if candidate_should_forward(Some(entries[i]),Liquidatable,slot,1,false,pending<1024) {
       pending+=1;entries[i].last_forwarded_slot=slot;forwards[i]+=1;
     }
   }
 }
 assert_eq!(forwards[0],100);assert_eq!(forwards[1024],0);
 println!("REPRODUCED: lexicographically last candidate starved for 100 slots; first 1,024 rearmed every slot");
}
'''
with tempfile.TemporaryDirectory() as d:
 f=Path(d)/'probe.rs'; f.write_text(code)
 subprocess.run(['rustc','--edition=2021',str(f),'-o',d+'/probe'],check=True)
 subprocess.run([d+'/probe'],check=True)
