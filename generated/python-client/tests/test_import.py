"""Smoke tests: the generated package imports and exposes its public API (Issue #1999)."""


def test_package_exports_public_api():
    import quorum_credit_client

    assert set(quorum_credit_client.__all__) == {"ClientConfig", "QuorumCreditClient"}
    assert hasattr(quorum_credit_client, "ClientConfig")
    assert hasattr(quorum_credit_client, "QuorumCreditClient")


def test_client_can_be_constructed_without_network():
    from quorum_credit_client import ClientConfig, QuorumCreditClient

    client = QuorumCreditClient(ClientConfig(base_url="https://api.example.invalid/"))

    assert client.base_url == "https://api.example.invalid/"
    assert client.timeout == 30
