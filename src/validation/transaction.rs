use crate::transaction::Transaction;

pub fn check_signature_batch(transactions: &[Transaction]) -> bool {
    let mut check = true;
    for (_idx, transaction) in transactions.iter().enumerate() {
        let t = transaction.clone();
        let (unverified, addr, public) = t.deconstruct();
        let signedtx = unverified.verify_unordered();
        match signedtx {
            Ok(tx) => {
                if addr != tx.sender() || public != tx.public_key() {
                    //debug!("Tx is not signed properly!");
                    check = false;
                    break;
                }
            },
            Err(e) => {
                //debug!("Tx is not signed properly!");
                check = false;
                break;
            },
        }
    }
    check
}
