"""
OpenMem — Agent-Agnostic LLM Interface.

Works with ANY LLM provider via litellm (OpenAI, Anthropic, Ollama, Gemini, etc.)
Falls back to heuristic keyword mode when no LLM is configured.

Usage:
    from core.llm import OpenMemLLM

    llm = OpenMemLLM()  # Auto-detects provider
    result = llm.summarize("daily memories text...")
"""

import os
import json
import logging
from typing import Dict, List, Optional, Any
from datetime import datetime
from pathlib import Path

logger = logging.getLogger(__name__)

OPENMEM_ROOT = Path(__file__).parent.parent
CONFIG_FILE = OPENMEM_ROOT / "config.json"


class OpenMemLLM:
    """
    Agent-agnostic LLM wrapper.

    Supports any provider via litellm:
    - OpenAI: gpt-4, gpt-3.5-turbo
    - Anthropic: claude-sonnet, claude-opus
    - Ollama: llama3, mistral, qwen
    - Gemini: gemini-pro
    - Any OpenAI-compatible endpoint

    Falls back to heuristic mode when no LLM is configured.
    """

    def __init__(self, config: Dict = None):
        self.config = config or self._load_config()
        self._client = None
        self._available = False
        # Static, network-free initialization. Availability here means
        # "configured and plausible" (litellm importable + provider + API
        # key resolvable); the first real completion is the only time we
        # ever touch the network. See _init_client.
        self._litellm = None
        self._init_client()

    def _load_config(self) -> Dict:
        """Load configuration."""
        if CONFIG_FILE.exists():
            try:
                with open(CONFIG_FILE, "r") as f:
                    return json.load(f)
            except (json.JSONDecodeError, OSError):
                pass
        return {"llm": {"provider": "auto", "model": "auto"}}

    def _resolve_api_key_env(self) -> Optional[str]:
        """
        Resolve which environment variable holds the API key.

        Honors config llm.api_key_env when it names a concrete variable;
        "AUTO" probes the common provider keys in a fixed order.

        Returns:
            Environment variable name carrying a key, or None
        """
        llm_config = self.config.get("llm", {})
        api_key_env = llm_config.get("api_key_env", "AUTO")
        if api_key_env != "AUTO":
            return api_key_env if os.environ.get(api_key_env) else None

        for env_var in ["OPENAI_API_KEY", "ANTHROPIC_API_KEY",
                        "GEMINI_API_KEY", "OLLAMA_BASE_URL"]:
            if os.environ.get(env_var):
                return env_var
        return None

    def _init_client(self):
        """
        Initialize LLM client configuration — WITHOUT any network I/O.

        The historical implementation fired a live `litellm.completion`
        probe here, which blocked cycle startup on network timeouts and
        made import-time behavior environment-dependent. Availability is
        now a static check:

        - litellm importable?
        - provider resolved (explicit config or auto-detected from env)?
        - an API key (or OLLAMA_BASE_URL for local providers) present?

        All three → `_available = True` optimistically; the actual network
        call happens lazily on first use, and a hard failure there flips
        availability off so later calls degrade to heuristics without
        repeated stalls. Anything less → heuristic mode immediately.
        """
        llm_config = self.config.get("llm", {})
        provider = str(llm_config.get("provider", "auto")).lower()
        model = llm_config.get("model", "auto")

        try:
            import litellm
            self._litellm = litellm
        except ImportError:
            # litellm not installed — use heuristic mode
            self._available = False
            self._litellm = None
            return

        key_env = self._resolve_api_key_env()

        # Auto-detect provider from env evidence if not pinned in config
        if provider == "auto":
            if os.environ.get("OPENAI_API_KEY"):
                provider = "openai"
                if model == "auto":
                    model = "gpt-4o-mini"
            elif os.environ.get("ANTHROPIC_API_KEY"):
                provider = "anthropic"
                if model == "auto":
                    model = "claude-sonnet-4-20250514"
            elif os.environ.get("GEMINI_API_KEY") or os.environ.get("GOOGLE_API_KEY"):
                provider = "gemini"
                if model == "auto":
                    model = "gemini-pro"
            elif os.environ.get("OLLAMA_BASE_URL"):
                provider = "ollama"
                if model == "auto":
                    model = "llama3"
            else:
                # No provider configured — use heuristic fallback
                self._available = False
                return

        # Local providers need no key; hosted ones do. No key = no network,
        # ever (zero attempts even on first use).
        needs_key = provider != "ollama"
        if needs_key and not (key_env and os.environ.get(key_env)):
            self._available = False
            return

        if not needs_key and key_env != "OLLAMA_BASE_URL" \
                and not os.environ.get("OLLAMA_BASE_URL"):
            self._available = False
            return

        # Wire a non-standard key variable into litellm (standard variables
        # are picked up by litellm itself).
        if key_env and os.environ.get(key_env):
            key_value = os.environ[key_env]
            try:
                if "OPENAI" in key_env:
                    self._litellm.openai_key = key_value
                elif "ANTHROPIC" in key_env:
                    self._litellm.anthropic_key = key_value
                elif "GEMINI" in key_env or "GOOGLE" in key_env:
                    self._litellm.google_ai_studio_key = key_value
            except AttributeError:
                pass

        self._available = True
        self._provider = provider
        self._model = model
        self._model_name = f"{provider}/{model}"

    @property
    def is_available(self) -> bool:
        """Check if LLM is available."""
        return self._available

    @property
    def provider(self) -> str:
        """Get current provider."""
        return getattr(self, "_provider", "heuristic")

    @property
    def model(self) -> str:
        """Get current model."""
        return getattr(self, "_model", "heuristic")

    def chat(self, messages: List[Dict], max_tokens: int = 2000,
             temperature: float = 0.7) -> str:
        """
        Send a chat completion request.

        Args:
            messages: List of {"role": "user"|"assistant"|"system", "content": "..."}
            max_tokens: Max response tokens
            temperature: Response temperature

        Returns:
            Response text
        """
        if self._available and self._litellm:
            try:
                response = self._litellm.completion(
                    model=self._model_name,
                    messages=messages,
                    max_tokens=max_tokens,
                    temperature=temperature,
                )
                return response.choices[0].message.content.strip()
            except Exception as e:
                # First hard failure flips availability off so later calls
                # degrade to heuristics instead of re-stalling on the wire.
                logger.warning(f"[LLM] Completion failed ({e}); "
                               f"disabling LLM backend for this instance")
                self._available = False
                return self._heuristic_response(messages)

        # Heuristic fallback
        return self._heuristic_response(messages)

    def summarize(self, text: str, max_length: int = 200) -> str:
        """
        Summarize text content.

        Args:
            text: Text to summarize
            max_length: Max summary length

        Returns:
            Summary string
        """
        if self._available:
            return self.chat([
                {"role": "system", "content": "Summarize the following text concisely."},
                {"role": "user", "content": text[:4000]},
            ], max_tokens=max_length, temperature=0.3)

        # Heuristic: extract important sentences
        return self._heuristic_summarize(text, max_length)

    def generate_skill(self, pattern: Dict, existing_skills: List[Dict] = None) -> str:
        """
        Generate a skill definition from a pattern.

        Args:
            pattern: Pattern dict from pattern recognizer
            existing_skills: List of existing skill definitions

        Returns:
            Skill code as string
        """
        if self._available:
            existing_text = ""
            if existing_skills:
                existing_text = "\n\nExisting skills (avoid duplicating):\n"
                for s in existing_skills[:3]:
                    existing_text += f"- {s.get('name', 'unknown')}: triggers: {s.get('triggers', [])}\n"

            return self.chat([
                {"role": "system", "content": (
                    "You are an expert Python developer. Generate a complete skill module "
                    "for an AI agent memory system. The skill should detect trigger keywords "
                    "in user messages and provide helpful responses based on past interactions."
                )},
                {"role": "user", "content": (
                    f"Generate a Python skill module for this pattern:\n"
                    f"Pattern: {json.dumps(pattern, indent=2)[:1000]}\n"
                    f"{existing_text}"
                    f"\nReturn only the Python code for a learner module with:\n"
                    f"- should_activate(context) -> bool\n"
                    f"- execute(context) -> dict\n"
                    f"- get_metadata() -> dict"
                )},
            ], max_tokens=3000, temperature=0.5)

        # Heuristic: generate template-based skill
        return self._heuristic_skill(pattern)

    # Keys that mark a reflection payload as usable by the reflection
    # engine. A parsed JSON object carrying none of them is treated as a
    # malformed answer, not as an empty-but-valid reflection.
    _REFLECTION_KEYS = ("outcome", "what_went_well", "what_to_improve",
                        "facts_to_remember", "knowledge_gaps")

    def reflect(self, session_messages: List[Dict]) -> Dict:
        """
        Perform self-reflection on a conversation session.

        Args:
            session_messages: List of {"role": ..., "content": ...}

        Returns:
            Reflection dict with outcome / what_went_well / what_to_improve /
            facts_to_remember / knowledge_gaps

        Raises:
            ValueError: If the model returned unparseable JSON or a JSON
                value without any recognized reflection field. Callers
                (reflection_engine) treat this as the signal to fall back
                to heuristic mode with a logged warning.
        """
        if self._available:
            conversation = "\n".join(
                f"{m['role']}: {m['content'][:300]}"
                for m in session_messages[:20]
            )

            response = self.chat([
                {"role": "system", "content": (
                    "You are a self-reflection engine for an AI assistant. "
                    "Analyze the conversation and return JSON with:\n"
                    "- outcome: 'success', 'failure', or 'neutral'\n"
                    "- what_went_well: list of things that worked\n"
                    "- what_to_improve: list of improvements\n"
                    "- facts_to_remember: list of important facts from the user\n"
                    "- knowledge_gaps: topics the assistant didn't know about"
                )},
                {"role": "user", "content": f"Reflect on this conversation:\n{conversation}"},
            ], max_tokens=1000, temperature=0.3)

            return self._parse_reflection_payload(response)

        # Heuristic fallback
        return self._heuristic_reflection(session_messages)

    def _parse_reflection_payload(self, response: str) -> Dict:
        """
        Parse and shape-validate raw LLM reflection text.

        Args:
            response: Raw completion text expected to carry JSON

        Returns:
            Parsed reflection dict

        Raises:
            ValueError: On invalid JSON or a payload missing every
                recognized reflection key (includes a response snippet for
                diagnosability)
        """
        try:
            parsed = json.loads(response)
        except json.JSONDecodeError as e:
            snippet = (response or "")[:200]
            raise ValueError(
                f"LLM reflection returned invalid JSON ({e}): {snippet!r}"
            ) from e

        if not isinstance(parsed, dict) or not any(
                k in parsed for k in self._REFLECTION_KEYS):
            snippet = (response or "")[:200]
            raise ValueError(
                f"LLM reflection JSON lacks recognized keys "
                f"{list(self._REFLECTION_KEYS)}: {snippet!r}"
            )
        return parsed

    def extract_facts(self, text: str) -> Dict[str, str]:
        """
        Extract important facts from text.

        Args:
            text: Text to analyze

        Returns:
            Dict of {fact_key: fact_value}
        """
        if self._available:
            response = self.chat([
                {"role": "system", "content": (
                    "Extract important facts about the user from this text. "
                    "Return JSON: {\"fact_key\": \"fact_value\"}. "
                    "Keys like: user_name, current_project, company, location, preferences."
                )},
                {"role": "user", "content": text[:2000]},
            ], max_tokens=500, temperature=0.2)

            try:
                return json.loads(response)
            except json.JSONDecodeError:
                return {}

        # Heuristic
        return self._heuristic_facts(text)

    def profile_user(self, messages: List[Dict]) -> Dict:
        """
        Analyze user communication style from messages.

        Args:
            messages: User messages

        Returns:
            Profile dict with formality, verbosity, topics, etc.
        """
        if self._available:
            user_texts = "\n".join(
                m.get("content", "")[:200]
                for m in messages
                if m.get("role") == "user"
            )[:3000]

            response = self.chat([
                {"role": "system", "content": (
                    "Analyze this user's communication style. Return JSON:\n"
                    "{\n"
                    '  "formality": 0.0-1.0,\n'
                    '  "verbosity": 0.0-1.0,\n'
                    '  "emoji_usage": 0.0-1.0,\n'
                    '  "preferred_response": "concise"|"detailed"|"structured",\n'
                    '  "topics_of_interest": ["topic1", "topic2"],\n'
                    '  "communication_tips": ["tip1", "tip2"]\n'
                    "}"
                )},
                {"role": "user", "content": f"Analyze this user's communication style:\n{user_texts}"},
            ], max_tokens=500, temperature=0.3)

            try:
                return json.loads(response)
            except json.JSONDecodeError:
                return {}

        return {}

    # ------------------------------------------------------------------
    # Heuristic Fallbacks (when no LLM is configured)
    # ------------------------------------------------------------------

    def _heuristic_response(self, messages: List[Dict]) -> str:
        """Generate response without LLM."""
        last_user = ""
        for m in reversed(messages):
            if m.get("role") == "user":
                last_user = m.get("content", "")
                break

        if not last_user:
            return ""

        # Simple keyword matching
        lower = last_user.lower()
        if any(kw in lower for kw in ["hello", "hi ", "hey"]):
            return "Hello! How can I help you today?"
        if any(kw in lower for kw in ["thanks", "thank", "great", "perfect"]):
            return "You're welcome! Let me know if you need anything else."
        if any(kw in lower for kw in ["who are you", "what are you"]):
            return "I'm an AI assistant with persistent memory via OpenMem."

        return ""

    def _heuristic_summarize(self, text: str, max_length: int = 200) -> str:
        """Summarize without LLM."""
        sentences = text.replace(". ", ".\n").split("\n")
        # Take first N sentences that fit max_length
        summary = []
        total = 0
        for s in sentences:
            s = s.strip()
            if len(s) > 10:
                summary.append(s)
                total += len(s)
                if total >= max_length:
                    break
        return ". ".join(summary)

    def _heuristic_skill(self, pattern: Dict) -> str:
        """Generate skill template without LLM."""
        keywords = pattern.get("high_freq_keywords", [])[:5]
        return f"""# Auto-generated skill (heuristic mode — configure LLM for full generation)

Triggers: {', '.join(keywords)}

def should_activate(context):
    msg = context.get("message", "").lower()
    return any(kw in msg for kw in {keywords})

def execute(context):
    return {{"response": f"Skill activated for: {{context.get('message', '')[:100]}}"}}
"""

    def _heuristic_reflection(self, messages: List[Dict]) -> Dict:
        """Reflect without LLM."""
        user_msgs = [m for m in messages if m.get("role") == "user"]
        if not user_msgs:
            return {"outcome": "neutral", "what_went_well": [], "what_to_improve": []}

        last = user_msgs[-1].get("content", "").lower()
        if any(kw in last for kw in ["thanks", "perfect", "great"]):
            outcome = "success"
        elif any(kw in last for kw in ["doesn't work", "wrong", "still"]):
            outcome = "failure"
        else:
            outcome = "neutral"

        return {
            "outcome": outcome,
            "what_went_well": ["Completed the interaction"],
            "what_to_improve": [],
            "facts_to_remember": [],
        }

    def _heuristic_facts(self, text: str) -> Dict[str, str]:
        """Extract facts without LLM."""
        import re
        facts = {}
        lower = text.lower()

        for pattern, key in [
            (r"my name is (\w+)", "user_name"),
            (r"i'm (\w+)", "user_name"),
            (r"i am (\w+)", "user_name"),
            (r"call me (\w+)", "user_name"),
            (r"working on (\w+)", "current_project"),
            (r"project (\w+)", "current_project"),
        ]:
            match = re.search(pattern, lower)
            if match:
                facts[key] = match.group(1)

        return facts

    def get_status(self) -> Dict:
        """Get LLM status."""
        return {
            "available": self._available,
            "provider": getattr(self, "_provider", "none"),
            "model": getattr(self, "_model", "none"),
            "fallback": "heuristic" if not self._available else "none",
        }


# Singleton
_llm_instance = None


def get_llm(config: Dict = None) -> OpenMemLLM:
    """Get or create LLM singleton."""
    global _llm_instance
    if _llm_instance is None:
        _llm_instance = OpenMemLLM(config)
    return _llm_instance
