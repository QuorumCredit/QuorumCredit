"""Unit tests for the QuorumCredit Python SDK client."""
from __future__ import annotations

import asyncio
from typing import Any
from unittest.mock import AsyncMock, MagicMock, patch

import pytest

from quorum_credit.client import (
    ClientConfig,
    Config,
    LoanRecord,
    QuorumCreditClient,
    VouchRecord,
    stroops_to_xlm,
    xlm_to_stroops,
)

# ── Fixtures ──────────────────────────────────────────────────────────────────

CONTRACT_ID = "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAABSC4"
RPC_URL = "https://soroban-testnet.stellar.org:443"
NETWORK = "Test SDF Network ; September 2015"
VOUCHER = "GBVOUCHER111111111111111111111111111111111111111111111111"
BORROWER = "GBBORROWER11111111111111111111111111111111111111111111111"
TOKEN = CONTRACT_ID
ADMIN = "GBADMIN1111111111111111111111111111111111111111111111111"
TX_HASH = "deafbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"


def make_keypair() -> MagicMock:
    kp = MagicMock()
    kp.public_key = VOUCHER
    return kp


def make_client() -> tuple[QuorumCreditClient, MagicMock]:
    keypair = make_keypair()
    config = ClientConfig(
        contract_id=CONTRACT_ID,
        rpc_url=RPC_URL,
        network_passphrase=NETWORK,
        keypair=keypair,
    )
    client = QuorumCreditClient(config)

    # Stub the soroban server so no real RPC calls are made
    server = MagicMock()
    account = MagicMock()
    account.increment_sequence_number = MagicMock()
    server.load_account.return_value = account

    # Build transaction returns a mock tx
    tx = MagicMock()
    builder = MagicMock()
    builder.append_invoke_contract_function_op.return_value = builder
    builder.set_timeout.return_value = builder
    builder.build.return_value = tx

    import stellar_sdk  # type: ignore[import]

    with patch.object(stellar_sdk, "TransactionBuilder", return_value=builder):
        client.server = server

    return client, server


# ── Helper function tests ─────────────────────────────────────────────────────


class TestXlmToStroops:
    def test_whole_xlm(self) -> None:
        assert xlm_to_stroops(1.0) == 10_000_000

    def test_100_xlm(self) -> None:
        assert xlm_to_stroops(100.0) == 1_000_000_000

    def test_fractional(self) -> None:
        assert xlm_to_stroops(0.1) == 1_000_000

    def test_minimum_unit(self) -> None:
        assert xlm_to_stroops(0.0000001) == 1


class TestStroopsToXlm:
    def test_basic(self) -> None:
        assert stroops_to_xlm(10_000_000) == 1.0

    def test_string_input(self) -> None:
        assert stroops_to_xlm("1000000000") == 100.0

    def test_minimum(self) -> None:
        assert stroops_to_xlm(1) == pytest.approx(1e-7)


# ── Write-path tests ──────────────────────────────────────────────────────────


class TestWriteMethods:
    """Tests for methods that submit transactions (non-readonly)."""

    def _setup(self) -> tuple[QuorumCreditClient, MagicMock]:
        client, server = make_client()
        send_result = MagicMock()
        send_result.hash = TX_HASH
        server.send_transaction.return_value = send_result
        return client, server

    @pytest.mark.asyncio
    async def test_vouch(self) -> None:
        client, server = self._setup()
        # Patch _invoke to avoid TransactionBuilder complexity
        client._invoke = AsyncMock(return_value=TX_HASH)  # type: ignore[method-assign]
        result = await client.vouch(VOUCHER, BORROWER, 1_000_000_000, TOKEN)
        assert result == TX_HASH
        client._invoke.assert_awaited_once_with("vouch", False, VOUCHER, BORROWER, 1_000_000_000, TOKEN)

    @pytest.mark.asyncio
    async def test_batch_vouch(self) -> None:
        client, _ = self._setup()
        client._invoke = AsyncMock(return_value=TX_HASH)  # type: ignore[method-assign]
        result = await client.batch_vouch(VOUCHER, [BORROWER], [1_000_000_000], TOKEN)
        assert result == TX_HASH

    @pytest.mark.asyncio
    async def test_batch_vouch_length_mismatch_raises(self) -> None:
        client, _ = self._setup()
        with pytest.raises(ValueError, match="same length"):
            await client.batch_vouch(VOUCHER, [BORROWER, BORROWER], [100], TOKEN)

    @pytest.mark.asyncio
    async def test_increase_stake(self) -> None:
        client, _ = self._setup()
        client._invoke = AsyncMock(return_value=TX_HASH)  # type: ignore[method-assign]
        result = await client.increase_stake(VOUCHER, BORROWER, 100_000_000, TOKEN)
        assert result == TX_HASH
        client._invoke.assert_awaited_once_with("increase_stake", False, VOUCHER, BORROWER, 100_000_000, TOKEN)

    @pytest.mark.asyncio
    async def test_decrease_stake(self) -> None:
        client, _ = self._setup()
        client._invoke = AsyncMock(return_value=TX_HASH)  # type: ignore[method-assign]
        result = await client.decrease_stake(VOUCHER, BORROWER, 500_000_000, TOKEN)
        assert result == TX_HASH

    @pytest.mark.asyncio
    async def test_withdraw_vouch(self) -> None:
        client, _ = self._setup()
        client._invoke = AsyncMock(return_value=TX_HASH)  # type: ignore[method-assign]
        result = await client.withdraw_vouch(VOUCHER, BORROWER, TOKEN)
        assert result == TX_HASH
        client._invoke.assert_awaited_once_with("withdraw_vouch", False, VOUCHER, BORROWER, TOKEN)

    @pytest.mark.asyncio
    async def test_request_loan(self) -> None:
        client, _ = self._setup()
        client._invoke = AsyncMock(return_value=TX_HASH)  # type: ignore[method-assign]
        result = await client.request_loan(BORROWER, 500_000_000, 1_000_000_000, "Business", TOKEN)
        assert result == TX_HASH

    @pytest.mark.asyncio
    async def test_repay(self) -> None:
        client, _ = self._setup()
        client._invoke = AsyncMock(return_value=TX_HASH)  # type: ignore[method-assign]
        result = await client.repay(BORROWER, 510_000_000)
        assert result == TX_HASH
        client._invoke.assert_awaited_once_with("repay", False, BORROWER, 510_000_000)

    @pytest.mark.asyncio
    async def test_slash(self) -> None:
        client, _ = self._setup()
        client._invoke = AsyncMock(return_value=TX_HASH)  # type: ignore[method-assign]
        result = await client.slash([ADMIN], BORROWER)
        assert result == TX_HASH

    @pytest.mark.asyncio
    async def test_vote_slash(self) -> None:
        client, _ = self._setup()
        client._invoke = AsyncMock(return_value=TX_HASH)  # type: ignore[method-assign]
        result = await client.vote_slash(VOUCHER, BORROWER, True)
        assert result == TX_HASH
        client._invoke.assert_awaited_once_with("vote_slash", False, VOUCHER, BORROWER, True)

    @pytest.mark.asyncio
    async def test_execute_slash_vote(self) -> None:
        client, _ = self._setup()
        client._invoke = AsyncMock(return_value=TX_HASH)  # type: ignore[method-assign]
        result = await client.execute_slash_vote(BORROWER)
        assert result == TX_HASH
        client._invoke.assert_awaited_once_with("execute_slash_vote", False, BORROWER)

    @pytest.mark.asyncio
    async def test_pause(self) -> None:
        client, _ = self._setup()
        client._invoke = AsyncMock(return_value=TX_HASH)  # type: ignore[method-assign]
        result = await client.pause([ADMIN])
        assert result == TX_HASH
        client._invoke.assert_awaited_once_with("pause", False, [ADMIN])

    @pytest.mark.asyncio
    async def test_unpause(self) -> None:
        client, _ = self._setup()
        client._invoke = AsyncMock(return_value=TX_HASH)  # type: ignore[method-assign]
        result = await client.unpause([ADMIN])
        assert result == TX_HASH

    @pytest.mark.asyncio
    async def test_update_config_yield_only(self) -> None:
        client, _ = self._setup()
        client._invoke = AsyncMock(return_value=TX_HASH)  # type: ignore[method-assign]
        result = await client.update_config([ADMIN], yield_bps=300)
        assert result == TX_HASH
        client._invoke.assert_awaited_once_with("update_config", False, [ADMIN], 300, None)

    @pytest.mark.asyncio
    async def test_update_config_both_params(self) -> None:
        client, _ = self._setup()
        client._invoke = AsyncMock(return_value=TX_HASH)  # type: ignore[method-assign]
        result = await client.update_config([ADMIN], yield_bps=300, slash_bps=5000)
        assert result == TX_HASH
        client._invoke.assert_awaited_once_with("update_config", False, [ADMIN], 300, 5000)


# ── Read-path tests ───────────────────────────────────────────────────────────


class TestReadMethods:
    """Tests for readonly methods that simulate transactions."""

    @pytest.mark.asyncio
    async def test_get_loan_returns_none_when_no_result(self) -> None:
        client, _ = make_client()
        client._invoke = AsyncMock(return_value=None)  # type: ignore[method-assign]
        result = await client.get_loan(BORROWER)
        assert result is None

    @pytest.mark.asyncio
    async def test_get_vouches_returns_empty_when_no_result(self) -> None:
        client, _ = make_client()
        client._invoke = AsyncMock(return_value=None)  # type: ignore[method-assign]
        result = await client.get_vouches(BORROWER)
        assert result == []

    @pytest.mark.asyncio
    async def test_is_eligible_returns_false_on_no_result(self) -> None:
        client, _ = make_client()
        client._invoke = AsyncMock(return_value=None)  # type: ignore[method-assign]
        result = await client.is_eligible(BORROWER, 1_000_000_000, TOKEN)
        assert result is False

    @pytest.mark.asyncio
    async def test_loan_status_returns_none_on_missing(self) -> None:
        client, _ = make_client()
        client._invoke = AsyncMock(return_value=None)  # type: ignore[method-assign]
        status = await client.loan_status(BORROWER)
        assert status == "None"

    @pytest.mark.asyncio
    async def test_get_admins_returns_empty_on_missing(self) -> None:
        client, _ = make_client()
        client._invoke = AsyncMock(return_value=None)  # type: ignore[method-assign]
        admins = await client.get_admins()
        assert admins == []

    @pytest.mark.asyncio
    async def test_total_vouched_returns_zero_on_missing(self) -> None:
        client, _ = make_client()
        client._invoke = AsyncMock(return_value=None)  # type: ignore[method-assign]
        total = await client.total_vouched(BORROWER)
        assert total == "0"

    @pytest.mark.asyncio
    async def test_get_fee_treasury_returns_zero_on_missing(self) -> None:
        client, _ = make_client()
        client._invoke = AsyncMock(return_value=None)  # type: ignore[method-assign]
        fee = await client.get_fee_treasury()
        assert fee == "0"

    @pytest.mark.asyncio
    async def test_get_loan_parses_record(self) -> None:
        client, _ = make_client()
        raw = {
            "id": "1",
            "borrower": BORROWER,
            "amount": 500_000_000,
            "amount_repaid": 0,
            "total_yield": 10_000_000,
            "status": "Active",
            "created_at": 1000,
            "deadline": 2000,
            "loan_purpose": "Business",
        }
        client._invoke = AsyncMock(return_value=raw)  # type: ignore[method-assign]
        # Bypass _native since value is already a dict
        client._native = lambda v: v  # type: ignore[method-assign]
        result = await client.get_loan(BORROWER)
        assert result is not None
        assert result.status == "Active"
        assert result.borrower == BORROWER

    @pytest.mark.asyncio
    async def test_get_vouches_parses_records(self) -> None:
        client, _ = make_client()
        raw = [{"voucher": VOUCHER, "stake": 1_000_000_000, "vouch_timestamp": 999, "token": TOKEN}]
        client._invoke = AsyncMock(return_value=raw)  # type: ignore[method-assign]
        client._native = lambda v: v  # type: ignore[method-assign]
        result = await client.get_vouches(BORROWER)
        assert len(result) == 1
        assert result[0].voucher == VOUCHER
        assert result[0].stake == 1_000_000_000

    @pytest.mark.asyncio
    async def test_get_config_parses_config(self) -> None:
        client, _ = make_client()
        raw = {
            "admins": [ADMIN],
            "admin_threshold": 1,
            "token": TOKEN,
            "allowed_tokens": [],
            "yield_bps": 200,
            "slash_bps": 5000,
            "min_loan_amount": 100_000,
            "max_loan_amount": 1_000_000_000_000,
            "loan_duration": 2_592_000,
        }
        client._invoke = AsyncMock(return_value=raw)  # type: ignore[method-assign]
        client._native = lambda v: v  # type: ignore[method-assign]
        config = await client.get_config()
        assert config.admins == [ADMIN]
        assert config.yield_bps == 200
