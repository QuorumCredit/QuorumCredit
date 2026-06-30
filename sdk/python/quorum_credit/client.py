from __future__ import annotations

import importlib
from dataclasses import dataclass
from typing import Any, Optional

stellar_sdk: Any = importlib.import_module("stellar_sdk")


@dataclass(frozen=True)
class ClientConfig:
    contract_id: str
    rpc_url: str
    network_passphrase: str
    keypair: Any


@dataclass(frozen=True)
class LoanRecord:
    id: str
    borrower: str
    amount: int | str
    amount_repaid: int | str
    total_yield: int | str
    status: str
    created_at: int
    deadline: int
    loan_purpose: str


@dataclass(frozen=True)
class VouchRecord:
    voucher: str
    stake: int | str
    vouch_timestamp: int
    token: str


@dataclass(frozen=True)
class Config:
    admins: list[str]
    admin_threshold: int
    token: str
    allowed_tokens: list[str]
    yield_bps: int
    slash_bps: int
    min_loan_amount: int | str
    max_loan_amount: int | str
    loan_duration: int


class QuorumCreditClient:
    def __init__(self, config: ClientConfig) -> None:
        self.config = config
        self.server: Any = stellar_sdk.SorobanServer(config.rpc_url)

    async def initialize(
        self,
        deployer: str,
        admins: list[str],
        admin_threshold: int,
        token: str,
    ) -> str:
        return str(await self._invoke("initialize", False, deployer, admins, admin_threshold, token))

    async def vouch(self, voucher: str, borrower: str, stake: int | str, token: str) -> str:
        return str(await self._invoke("vouch", False, voucher, borrower, stake, token))

    async def batch_vouch(
        self,
        voucher: str,
        borrowers: list[str],
        stakes: list[int | str],
        token: str,
    ) -> str:
        if len(borrowers) != len(stakes):
            raise ValueError("borrowers and stakes must have the same length")
        return str(await self._invoke("batch_vouch", False, voucher, borrowers, stakes, token))

    async def request_loan(
        self,
        borrower: str,
        amount: int | str,
        threshold: int | str,
        loan_purpose: str,
        token: str,
    ) -> str:
        return str(await self._invoke("request_loan", False, borrower, amount, threshold, loan_purpose, token))

    async def repay(self, borrower: str, payment: int | str) -> str:
        return str(await self._invoke("repay", False, borrower, payment))

    async def slash(self, admin_signers: list[str], borrower: str) -> str:
        return str(await self._invoke("slash", False, admin_signers, borrower))

    async def get_loan(self, borrower: str) -> Optional[LoanRecord]:
        result = await self._invoke("get_loan", True, borrower)
        return self._parse_loan_record(result) if result else None

    async def get_vouches(self, borrower: str) -> list[VouchRecord]:
        result = await self._invoke("get_vouches", True, borrower)
        return self._parse_vouch_records(result) if result else []

    async def is_eligible(self, borrower: str, threshold: int | str, token: str) -> bool:
        return bool(await self._invoke("is_eligible", True, borrower, threshold, token))

    async def get_config(self) -> Config:
        return self._parse_config(await self._invoke("get_config", True))

    async def get_dynamic_min_stake(self, borrower: str) -> int:
        """Get the effective (dynamic) minimum stake required for a specific borrower.

        The dynamic minimum stake takes the admin-configured base min_stake and applies
        a reduction based on the borrower's credit tier. Borrowers with higher credit
        tiers (Good, VeryGood, Excellent) earn a discount on the minimum stake required.

        Returns 0 when no minimum stake is configured.
        The returned value is always <= the base min_stake.

        Args:
            borrower: The borrower address to calculate the dynamic min stake for.

        Returns:
            Effective minimum stake in stroops (1 XLM = 10_000_000 stroops).
        """
        result = await self._invoke("get_dynamic_min_stake", True, borrower)
        return int(self._native(result)) if result is not None else 0

    async def _invoke(self, name: str, readonly: bool, *args: object) -> Any:
        if readonly:
            tx = await self._build_transaction(name, *args)
            result = self.server.simulate_transaction(tx)
            if getattr(result, "error", None):
                raise RuntimeError(result.error)
            results = getattr(result, "results", None) or []
            return results[0].result.retval if results else None

        tx = await self._build_transaction(name, *args)
        result = self.server.send_transaction(tx)
        return str(result.hash)

    async def _build_transaction(self, name: str, *args: object) -> Any:
        account = self.server.load_account(self.config.keypair.public_key)
        builder = stellar_sdk.TransactionBuilder(
            account,
            base_fee="100",
            network_passphrase=self.config.network_passphrase,
        )
        return (
            builder.append_invoke_contract_function_op(self.config.contract_id, name, list(args))
            .set_timeout(30)
            .build()
        )

    def _parse_loan_record(self, value: Any) -> LoanRecord:
        native = self._native(value)
        return LoanRecord(
            id=str(native.get("id", "")),
            borrower=str(native.get("borrower", "")),
            amount=native.get("amount", "0"),
            amount_repaid=native.get("amount_repaid", "0"),
            total_yield=native.get("total_yield", "0"),
            status=str(native.get("status", "")),
            created_at=int(native.get("created_at", 0)),
            deadline=int(native.get("deadline", 0)),
            loan_purpose=str(native.get("loan_purpose", "")),
        )

    def _parse_vouch_records(self, value: Any) -> list[VouchRecord]:
        native = self._native(value)
        if not isinstance(native, list):
            return []
        return [
            VouchRecord(
                voucher=str(item.get("voucher", "")),
                stake=item.get("stake", "0"),
                vouch_timestamp=int(item.get("vouch_timestamp", 0)),
                token=str(item.get("token", "")),
            )
            for item in native
            if isinstance(item, dict)
        ]

    def _parse_config(self, value: Any) -> Config:
        native = self._native(value)
        return Config(
            admins=list(native.get("admins", [])),
            admin_threshold=int(native.get("admin_threshold", 0)),
            token=str(native.get("token", "")),
            allowed_tokens=list(native.get("allowed_tokens", [])),
            yield_bps=int(native.get("yield_bps", 0)),
            slash_bps=int(native.get("slash_bps", 0)),
            min_loan_amount=native.get("min_loan_amount", "0"),
            max_loan_amount=native.get("max_loan_amount", "0"),
            loan_duration=int(native.get("loan_duration", 0)),
        )

    def _native(self, value: Any) -> Any:
        converter = getattr(stellar_sdk, "scval_to_native", None)
        return converter(value) if converter else value
