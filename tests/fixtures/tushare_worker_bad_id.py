#!/usr/bin/env python3
from __future__ import annotations

import json
import sys


for _line in sys.stdin:
    print(json.dumps({"id": "wrong", "type": "end", "row_count": 0}), flush=True)
