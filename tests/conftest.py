import pytest


@pytest.fixture
def sample_dsn() -> str:
    return "postgresql://postgres:postgres@localhost:5432/stock_cache"
