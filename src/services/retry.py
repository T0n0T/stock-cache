import asyncio
import random
from collections.abc import Awaitable, Callable

from domain.errors import RetryableProviderError


async def with_retries(
    operation: Callable[[], Awaitable[object]],
    max_retries: int,
    base_delay: float,
    backoff_factor: float,
    jitter: float,
) -> object:
    attempt = 0
    while True:
        try:
            return await operation()
        except RetryableProviderError:
            attempt += 1
            if attempt > max_retries:
                raise
            delay = base_delay * (backoff_factor ** (attempt - 1))
            delay += random.uniform(0, jitter)
            await asyncio.sleep(delay)
