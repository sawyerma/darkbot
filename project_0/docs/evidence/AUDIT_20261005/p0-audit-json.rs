//! Audit-only repro: copy into darkbot-runner/src/bin in an isolated audit checkout.
use darkbot_project0::economics::{self, CandidateInput, Path};
use darkbot_project0::funding::FundingClass;
fn main() {
    for path in std::env::args().skip(1) {
        let raw=std::fs::read_to_string(path).unwrap();
        let tx: solana_transaction_status::EncodedConfirmedTransactionWithStatusMeta=serde_json::from_str(&raw).unwrap();
        let v=serde_json::to_value(tx).unwrap();
        assert!(v.pointer("/transaction/transaction/message/accountKeys").is_none());
        assert!(v.pointer("/transaction/transaction/message/instructions").is_none());
        assert!(v.pointer("/transaction/message/instructions").unwrap().as_array().unwrap().len()>0);
        println!("REPRODUCED slot {}: winner.rs JSON pointers miss all instructions",v["slot"]);
    }
    let costs=economics::rpc_message_failure_costs(Some(1_000));
    let mut c=CandidateInput{
        gross_seized_realizable_usd_micro:Some(100_300_000),repay_usd_micro:100_000_000,
        protocol_fees_usd_micro:0,flat_sol_fee_usd_micro:120_000,swap_fees_and_impact_usd_micro:0,
        transaction_fee_usd_micro:Some(1_000),jito_tip_usd_micro:0,rent_and_token_usd_micro:Some(0),
        required_capital_usd_micro:0,path:Path::Rpc,funding:FundingClass::ZeroPrefundSameTx};
    let quote=economics::evaluate(&c,&costs);
    c.gross_seized_realizable_usd_micro=Some(100_099_400); // permitted 20 bps adverse execution
    let floor=economics::evaluate(&c,&costs);
    assert!(matches!(quote,economics::Evaluation::Known{net:178000,..}));
    assert!(matches!(floor,economics::Evaluation::Known{net:-22600,..}));
    println!("REPRODUCED quote net +178000 USD-micro, allowed output-floor net -22600 USD-micro (repayment still covered)");
}
