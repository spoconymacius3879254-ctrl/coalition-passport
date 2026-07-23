import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
SCRIPT = (ROOT / "scripts" / "devnet-demo.sh").read_text()


class DevnetScriptTests(unittest.TestCase):
    def test_default_mode_is_check_only(self):
        self.assertIn('MODE="${1:---check}"', SCRIPT)
        self.assertIn("check-only mode: no transaction was signed or submitted", SCRIPT)

    def test_execute_requires_devnet_and_minimum_balance(self):
        self.assertIn('RPC_URL="https://api.devnet.solana.com"', SCRIPT)
        self.assertIn('EXPECTED_GENESIS="EtWTRABZaYq6iMfeYKouRu166VU2xqa1wcaWoxPkrZBG"', SCRIPT)
        self.assertIn("MINIMUM_LAMPORTS=2300000000", SCRIPT)
        self.assertIn('BALANCE_LAMPORTS="${BALANCE_TEXT%% *}"', SCRIPT)
        self.assertNotIn("mainnet", SCRIPT.lower())

    def test_signers_are_explicit_devnet_only_files(self):
        for name in (
            ".anchor/devnet-deployer.json",
            ".anchor/devnet-merchant.json",
            ".anchor/devnet-customer.json",
        ):
            self.assertIn(name, SCRIPT)
        self.assertNotIn(".config/solana/id.json", SCRIPT)
        self.assertNotIn("seed phrase", SCRIPT.lower())

    def test_complete_flow_and_evidence_are_present(self):
        for operation in (
            "program deploy", "initialize-coalition", "register-merchant",
            "create-passport", "record-receipt", "redeem", "final-status",
            "complete.json",
        ):
            self.assertIn(operation, SCRIPT)


if __name__ == "__main__":
    unittest.main()
