"""
OpenMem Core — Shared AI-powered components.

These modules provide the actual intelligence:
- LLM interface (provider-agnostic via litellm)
- Memory consolidation (real summarization)
- Skill generation (real code generation)
- Reflection (real self-evaluation)
- User profiling (real embedding-based analysis)
"""

from .llm import OpenMemLLM, get_llm

__all__ = ["OpenMemLLM", "get_llm"]
