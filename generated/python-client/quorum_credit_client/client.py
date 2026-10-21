"""
QuorumCredit API Client (Python)

Auto-generated from OpenAPI specification.
Do not edit manually.
"""

import json
from typing import Any, Dict, Optional
import requests
from dataclasses import dataclass
from datetime import datetime


@dataclass
class ClientConfig:
    base_url: str
    timeout: Optional[int] = None
    headers: Optional[Dict[str, str]] = None


class QuorumCreditClient:
    """QuorumCredit API Client"""

    def __init__(self, config: ClientConfig):
        self.base_url = config.base_url
        self.timeout = config.timeout or 30
        self.headers = config.headers or {}

    def _request(self, method: str, path: str, **kwargs) -> Any:
        """Execute a raw HTTP request"""
        url = f"{self.base_url.rstrip('/')}{path}"
        headers = {**self.headers, **(kwargs.get('headers') or {})}

        response = requests.request(
            method=method,
            url=url,
            headers=headers,
            timeout=self.timeout,
            **{k: v for k, v in kwargs.items() if k != 'headers'}
        )
        response.raise_for_status()
        return response.json() if response.text else {}

    def get_health(self) -> Dict[str, str]:
        """Get health status"""
        return self._request("GET", "/health")

    def get_metrics(self) -> str:
        """Get Prometheus metrics"""
        response = requests.get(
            f"{self.base_url.rstrip('/')}/metrics",
            timeout=self.timeout
        )
        return response.text

    def search(self, **query_params) -> Dict[str, Any]:
        """Execute a faceted search"""
        params = {}
        if 'q' in query_params:
            params['q'] = query_params['q']
        if 'category' in query_params:
            params['category'] = query_params['category']
        if 'action' in query_params:
            params['action'] = query_params['action']
        if 'startLedger' in query_params:
            params['startLedger'] = query_params['startLedger']
        if 'endLedger' in query_params:
            params['endLedger'] = query_params['endLedger']
        if 'limit' in query_params:
            params['limit'] = query_params['limit']
        if 'offset' in query_params:
            params['offset'] = query_params['offset']

        return self._request("GET", "/api/search", params=params)

    def get_search_stats(self) -> Dict[str, Any]:
        """Get search statistics"""
        return self._request("GET", "/api/search/stats")

    def get_search_patterns(self, limit: Optional[int] = None) -> Dict[str, Any]:
        """Get top search patterns"""
        path = "/api/search/patterns"
        params = {}
        if limit:
            params['limit'] = limit
        return self._request("GET", path, params=params)

    def register_webhook(self, url: str, events: list) -> Dict[str, Any]:
        """Register a webhook"""
        payload = {"url": url, "events": events}
        return self._request("POST", "/api/webhooks/register", json=payload)

    def list_webhooks(self) -> list:
        """List all webhooks"""
        return self._request("GET", "/api/webhooks")

    def get_webhook(self, webhook_id: str) -> Dict[str, Any]:
        """Get a webhook by ID"""
        return self._request("GET", f"/api/webhooks/{webhook_id}")

    def delete_webhook(self, webhook_id: str) -> Dict[str, str]:
        """Delete a webhook"""
        return self._request("DELETE", f"/api/webhooks/{webhook_id}")

    def get_webhook_deliveries(self, webhook_id: str) -> list:
        """Get webhook delivery attempts"""
        return self._request("GET", f"/api/webhooks/{webhook_id}/deliveries")

    def get_webhook_stats(self, webhook_id: str) -> Dict[str, Any]:
        """Get webhook delivery statistics"""
        return self._request("GET", f"/api/webhooks/{webhook_id}/stats")
