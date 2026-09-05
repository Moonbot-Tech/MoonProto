use super::*;

impl Client {
    pub(crate) fn balance_send_digest(&self, digest: u64) {
        let raw = crate::commands::balance::build_balance_digest(rand::random(), 0, digest);
        self.send_typed_domain_cmd(raw, Command::Balance);
    }

    // ====================================================================
    //  High-level Balance wrappers (Command::Balance, encrypted=true)
    //  Cover the Delphi MClient.SendBalanceCmd semantics.
    // ====================================================================

    /// Send `TRequestBalanceRefresh` (Balance CmdId=5, High).
    #[doc(hidden)]
    pub(crate) fn balance_request_refresh(&self) {
        let raw = crate::commands::balance::build_request_balance_refresh(rand::random());
        self.send_typed_domain_cmd(raw, Command::Balance);
    }
}
