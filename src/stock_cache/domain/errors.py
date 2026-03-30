class StockCacheError(Exception):
    """Base error for stock-cache."""


class RetryableProviderError(StockCacheError):
    """A provider call can be retried safely."""


class NonRetryableProviderError(StockCacheError):
    """A provider call should fail fast."""


class ConfigurationError(StockCacheError):
    """Environment configuration is missing or invalid."""
