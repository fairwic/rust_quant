import importlib.util
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).resolve().parents[1] / "scripts" / "bsc_meme_event_backtest.py"
spec = importlib.util.spec_from_file_location("bsc_meme_event_backtest", MODULE_PATH)
bt = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bt)


class BscRpcSourceTests(unittest.TestCase):
    def test_chainstack_nodes_payload_builds_bsc_rpc_url(self):
        payload = {
            "results": [
                {
                    "id": "ND-1",
                    "status": "running",
                    "network": "NW-other",
                    "details": {
                        "https_endpoint": "https://eth-mainnet.core.chainstack.com",
                        "auth_key": "eth-key",
                    },
                },
                {
                    "id": "ND-2",
                    "status": "running",
                    "network": "NW-bsc",
                    "details": {
                        "https_endpoint": "https://bsc-mainnet.core.chainstack.com/",
                        "auth_key": "bsc-key",
                        "api_namespaces": ["eth", "net", "web3"],
                    },
                },
            ]
        }

        rpc_url = bt.resolve_chainstack_rpc_url_from_nodes(payload)

        self.assertEqual(rpc_url, "https://bsc-mainnet.core.chainstack.com/bsc-key")

    def test_chainstack_nodes_payload_requires_running_node_with_auth_key(self):
        payload = {
            "results": [
                {
                    "id": "ND-1",
                    "status": "running",
                    "network": "bsc-mainnet",
                    "details": {
                        "https_endpoint": "https://bsc-mainnet.core.chainstack.com",
                    },
                }
            ]
        }

        with self.assertRaisesRegex(RuntimeError, "CHAINSTACK_BSC_NODE_NOT_FOUND"):
            bt.resolve_chainstack_rpc_url_from_nodes(payload)

    def test_rpc_archive_error_is_detected(self):
        err = RuntimeError(
            {
                "code": -32002,
                "message": "Archive, Debug and Trace requests are not available",
            }
        )

        self.assertTrue(bt.is_rpc_archive_required_error(err))


if __name__ == "__main__":
    unittest.main()
