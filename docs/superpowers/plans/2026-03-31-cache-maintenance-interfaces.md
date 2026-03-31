# Cache Maintenance Interfaces Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a date-based cache deletion CLI and a stats CLI that reports actual queryable trade-date segments from the local cache.

**Architecture:** Keep CLI parsing in `src/cli.py`, move deletion and stats behavior into focused use cases, and add repository methods for local date inventory and range deletion. Use the provider trade calendar to split stored dates into continuous trading-date segments without assuming calendar-day continuity.

**Tech Stack:** Python 3.13, Typer CLI, asyncpg/PostgreSQL, pytest, Tushare adapter

---
