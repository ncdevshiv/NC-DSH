"""
Automatic User Modeling for OpenMem.
Infers user preferences, habits, and communication style from conversation history.
"""

import re
import os
import json
import sqlite3
from datetime import datetime
from typing import Dict, List, Any, Optional, Tuple
from collections import defaultdict
import hashlib

from . import get_vector_db


class UserModel:
    """
    Automatic user profiling engine.
    
    Extracts and tracks:
    - Communication style (formal/casual, verbose/terse)
    - Preferences (topics of interest, tools used)
    - Habits (active times, interaction patterns)
    - Important facts (names, projects, deadlines)
    """
    
    def __init__(self, base_path: str = None):
        self.vector_db = get_vector_db()

        # Use centralized data/ directory
        if base_path is None:
            base_path = os.path.join(os.path.dirname(__file__), "..", "data", "usermodel")
        self.base_path = os.path.abspath(base_path)
        os.makedirs(self.base_path, exist_ok=True)
        
        self.profile_path = os.path.join(self.base_path, "user_profile.json")
        self.behavior_path = os.path.join(self.base_path, "behavior_patterns.json")
        
        # Load existing profile
        self.profile = self._load_profile()
        self.behavior_patterns = self._load_behaviors()
    
    def _load_profile(self) -> Dict:
        """Load existing user profile or create new."""
        if os.path.exists(self.profile_path):
            with open(self.profile_path, 'r') as f:
                return json.load(f)
        return {
            "name": None,
            "communication_style": {
                "formality": 0.5,  # 0=very casual, 1=very formal
                "verbosity": 0.5,  # 0=terse, 1=verbose
                "emoji_usage": 0.5,
                "preferred_response_length": "medium"
            },
            "topics_of_interest": [],  # Will be auto-populated
            "active_hours": {},  # hour -> message_count
            "channels_used": {},  # channel -> count
            "tools_used": {},  # tool -> count
            "important_facts": {},  # key -> {value, confidence, last_updated}
            "preferences": {},  # preference -> {value, confidence}
            "first_seen": datetime.now().isoformat(),
            "last_updated": datetime.now().isoformat()
        }
    
    def _load_behaviors(self) -> Dict:
        """Load behavior patterns."""
        if os.path.exists(self.behavior_path):
            with open(self.behavior_path, 'r') as f:
                return json.load(f)
        return {
            "greeting_patterns": [],
            "question_patterns": [],
            "task_patterns": [],
            "success_indicators": [],
            "frustration_indicators": []
        }
    
    def _save_profile(self):
        """Persist user profile to disk."""
        self.profile["last_updated"] = datetime.now().isoformat()
        with open(self.profile_path, 'w') as f:
            json.dump(self.profile, f, indent=2)
        
        # Also update vector DB
        self.vector_db.set_user_profile("_full_profile", json.dumps(self.profile), confidence=0.9)
    
    def _save_behaviors(self):
        """Persist behavior patterns."""
        with open(self.behavior_path, 'w') as f:
            json.dump(self.behavior_patterns, f, indent=2)
    
    def analyze_message(self, message: str, sender: str = None, channel: str = None,
                       response: str = None, metadata: Dict = None) -> Dict:
        """
        Analyze a single message and update user model.
        Uses LLM-based analysis when available, falls back to heuristic.

        Returns analysis results.
        """
        analysis = {
            "detected_style": {},
            "topics_extracted": [],
            "facts_extracted": [],
            "sentiment": "neutral"
        }

        message_lower = message.lower()

        # Try LLM-based analysis first
        try:
            from core.llm import get_llm
            llm = get_llm()
            if llm.is_available:
                llm_analysis = llm.profile_user([
                    {"role": "user", "content": message}
                ])
                if llm_analysis:
                    analysis["detected_style"] = {
                        "formality": llm_analysis.get("formality", 0.5),
                        "verbosity": llm_analysis.get("verbosity", 0.5),
                        "emoji_usage": llm_analysis.get("emoji_usage", 0),
                        "is_question": "?" in message,
                    }
                    analysis["topics_extracted"] = llm_analysis.get("topics_of_interest", [])
                    for topic in analysis["topics_extracted"]:
                        if topic not in self.profile["topics_of_interest"]:
                            self.profile["topics_of_interest"].append(topic)

                    # LLM-extracted facts
                    llm_facts = llm.extract_facts(message)
                    for key, value in llm_facts.items():
                        self._update_important_fact(key, value)
                    analysis["facts_extracted"] = list(llm_facts.keys())

                    # Still run heuristic topic extraction for coverage
                    heuristic_topics = self._extract_topics(message)
                    for t in heuristic_topics:
                        if t not in analysis["topics_extracted"]:
                            analysis["topics_extracted"].append(t)
                            if t not in self.profile["topics_of_interest"]:
                                self.profile["topics_of_interest"].append(t)

                    # Continue with non-style analysis (active hours, tools, etc.)
                    if channel:
                        hour = datetime.now().hour
                        self.profile["active_hours"][hour] = self.profile["active_hours"].get(hour, 0) + 1
                        self.profile["channels_used"][channel] = self.profile["channels_used"].get(channel, 0) + 1

                    tools = self._extract_tool_usage(message, metadata)
                    for tool in tools:
                        self.profile["tools_used"][tool] = self.profile["tools_used"].get(tool, 0) + 1

                    if response:
                        self._analyze_response_pattern(message, response)
                        sentiment = self._detect_sentiment(message, response)
                        analysis["sentiment"] = sentiment
                        if sentiment in ["frustrated", "impressed", "satisfied"]:
                            self._update_success_indicators(message, response, sentiment)

                    self._save_profile()
                    return analysis
        except (ImportError, Exception):
            pass

        # Heuristic fallback (original logic)
        return self._analyze_message_heuristic(
            analysis, message, sender, channel, response, metadata
        )

    def _analyze_message_heuristic(self, analysis, message, sender, channel, response, metadata):
        """Heuristic message analysis (original logic)."""
        message_lower = message.lower()

        # Detect communication style
        analysis["detected_style"] = self._analyze_style(message)

        # Update communication style averages
        for key, value in analysis["detected_style"].items():
            if key in self.profile["communication_style"]:
                current = self.profile["communication_style"][key]
                self.profile["communication_style"][key] = current * 0.8 + value * 0.2

        # Extract topics
        topics = self._extract_topics(message)
        analysis["topics_extracted"] = topics
        for topic in topics:
            if topic not in self.profile["topics_of_interest"]:
                self.profile["topics_of_interest"].append(topic)

        # Track active hours
        if channel:
            hour = datetime.now().hour
            self.profile["active_hours"][hour] = self.profile["active_hours"].get(hour, 0) + 1
            self.profile["channels_used"][channel] = self.profile["channels_used"].get(channel, 0) + 1

        # Extract facts
        facts = self._extract_facts(message)
        analysis["facts_extracted"] = facts
        for fact_key, fact_value in facts.items():
            self._update_important_fact(fact_key, fact_value)

        # Detect tools
        tools = self._extract_tool_usage(message, metadata)
        for tool in tools:
            self.profile["tools_used"][tool] = self.profile["tools_used"].get(tool, 0) + 1

        # Analyze response patterns
        if response:
            self._analyze_response_pattern(message, response)
            sentiment = self._detect_sentiment(message, response)
            analysis["sentiment"] = sentiment
            if sentiment in ["frustrated", "impressed", "satisfied"]:
                self._update_success_indicators(message, response, sentiment)

        self._save_profile()
        return analysis
    
    def _analyze_style(self, message: str) -> Dict:
        """Analyze communication style of a message."""
        words = message.split()
        char_count = len(message)
        
        # Formality indicators
        formal_words = ["therefore", "however", "moreover", "consequently", "shall", "would", "please", "kindly"]
        casual_words = ["hey", "yeah", "gonna", "wanna", "cool", "awesome", "lol", "btw", "FYI"]
        
        formal_count = sum(1 for w in words if w.lower() in formal_words)
        casual_count = sum(1 for w in words if w.lower() in casual_words)
        
        formality = min(1.0, formal_count / max(1, len(words) / 10))
        casual_score = min(1.0, casual_count / max(1, len(words) / 10))
        formality = formality - casual_score * 0.3  # Casual reduces formality
        
        # Emoji usage
        emoji_pattern = re.compile(r'[\U0001F600-\U0001F64F\U0001F300-\U0001F5FF]')
        emoji_count = len(emoji_pattern.findall(message))
        emoji_usage = min(1.0, emoji_count / max(1, len(words) / 5))
        
        # Verbosity (average words per message in conversation context)
        verbosity = min(1.0, len(words) / 50)  # 50 words = very verbose
        
        # Question detection
        is_question = "?" in message
        
        return {
            "formality": max(0, min(1, formality)),
            "emoji_usage": emoji_usage,
            "verbosity": verbosity,
            "is_question": is_question
        }
    
    def _extract_topics(self, message: str) -> List[str]:
        """Extract topics from message using keyword patterns."""
        topics = []
        message_lower = message.lower()
        
        topic_keywords = {
            "coding": ["code", "programming", "script", "function", "bug", "debug", "api", "python", "javascript"],
            "ai_ml": ["ai", "ml", "machine learning", "model", "training", "neural", "gpt", "llm", "hermes"],
            "data": ["database", "data", "sql", "query", "table", "schema", "vector"],
            "web": ["web", "http", "url", "browser", "html", "css", "frontend", "backend"],
            "devops": ["deploy", "docker", "kubernetes", "ci/cd", "pipeline", "server", "cloud"],
            "automation": ["automate", "script", "workflow", "schedule", "cron", "task"],
            "chatbots": ["chatbot", "telegram", "discord", "whatsapp", "messaging", "bot"],
            "research": ["research", "search", "find", "look up", "investigate", "compare"],
            "personal": ["remember", "note", "remind", "tell me about", "my"],
            "creative": ["write", "story", "poem", "creative", "generate", "image"]
        }
        
        for topic, keywords in topic_keywords.items():
            if any(kw in message_lower for kw in keywords):
                if topic not in topics:
                    topics.append(topic)
        
        return topics
    
    def _extract_facts(self, message: str) -> Dict[str, str]:
        """Extract important facts using patterns."""
        facts = {}
        message_lower = message.lower()
        
        # Name patterns
        name_patterns = [
            r"my name is (\w+)",
            r"i'm (\w+)",
            r"i am (\w+)",
            r"call me (\w+)"
        ]
        # Name patterns, ordered most-explicit-first ("my name is X" beats
        # "i'm X"). First match wins: without this guard the generic
        # contraction pattern clobbers the explicit one, so "My name is
        # Charlie and I'm working on ..." extracted user_name="working".
        for pattern in name_patterns:
            match = re.search(pattern, message_lower)
            if match and "user_name" not in facts:
                facts["user_name"] = match.group(1).capitalize()
        
        # Project references
        project_patterns = [
            r"working on (\w+)",
            r"project (\w+)",
            r"my (\w+) project"
        ]
        for pattern in project_patterns:
            match = re.search(pattern, message_lower)
            if match:
                facts["current_project"] = match.group(1)
        
        # Company/org
        company_patterns = [
            r"at (\w+) (company|firm|lab)",
            r"i work (at|for) (\w+)",
            r"(\w+) team"
        ]
        for pattern in company_patterns:
            match = re.search(pattern, message_lower)
            if match:
                facts["company"] = match.group(2) if len(match.groups()) > 1 else match.group(1)
        
        # Location
        location_patterns = [
            r"in (new york|london|tokyo|mumbai|berlin|sf|nyc)",
            r"based in (\w+)",
            r"from (india|usa|uk|japan|canada|germany)"
        ]
        for pattern in location_patterns:
            match = re.search(pattern, message_lower)
            if match:
                facts["location"] = match.group(1)
        
        return facts
    
    def _extract_tool_usage(self, message: str, metadata: Dict = None) -> List[str]:
        """Extract tool usage from message or metadata."""
        tools = []
        message_lower = message.lower()
        
        tool_keywords = {
            "web_search": ["search", "google", "look up", "find on web"],
            "browser": ["browse", "open website", "visit", "scrape"],
            "code_editor": ["edit", "write code", "create file", "modify"],
            "terminal": ["run", "execute", "bash", "command", "shell"],
            "memory": ["remember", "search memory", "look up", "recall"],
            "messaging": ["send message", "notify", "message", "tell"]
        }
        
        for tool, keywords in tool_keywords.items():
            if any(kw in message_lower for kw in keywords):
                if tool not in tools:
                    tools.append(tool)
        
        # Check metadata for tool usage
        if metadata:
            if "tool_used" in metadata:
                tools.append(metadata["tool_used"])
            if "action" in metadata:
                tools.append(metadata["action"])
        
        return tools
    
    def _analyze_response_pattern(self, message: str, response: str):
        """Analyze how the user responds to different types of requests."""
        msg_lower = message.lower()
        resp_lower = response.lower()
        
        # Task completion patterns
        if any(kw in msg_lower for kw in ["build", "create", "make", "write"]):
            if "done" in resp_lower or "complete" in resp_lower or "finished" in resp_lower:
                self.behavior_patterns["task_patterns"].append({
                    "type": "task_completion",
                    "message_sample": message[:100],
                    "response_sample": response[:100],
                    "timestamp": datetime.now().isoformat()
                })
        
        # Question patterns
        if "?" in message:
            if len(response) > 100:  # Detailed answer
                self.behavior_patterns["question_patterns"].append({
                    "type": "detailed_answer",
                    "message_sample": message[:100],
                    "timestamp": datetime.now().isoformat()
                })
        
        # Keep only recent patterns
        max_patterns = 50
        for key in self.behavior_patterns:
            if len(self.behavior_patterns[key]) > max_patterns:
                self.behavior_patterns[key] = self.behavior_patterns[key][-max_patterns:]
        
        self._save_behaviors()
    
    def _detect_sentiment(self, message: str, response: str) -> str:
        """Detect user sentiment from message and response pair."""
        msg_lower = message.lower()
        resp_lower = response.lower()
        
        frustration_signals = ["again", "still not", "doesn't work", "wrong", "terrible", "frustrated", "annoying"]
        success_signals = ["perfect", "great", "thanks", "awesome", "works", "love it", "nice", "good"]
        
        frustration_count = sum(1 for s in frustration_signals if s in msg_lower or s in resp_lower)
        success_count = sum(1 for s in success_signals if s in msg_lower or s in resp_lower)
        
        if frustration_count > success_count:
            return "frustrated"
        elif success_count > frustration_count:
            return "satisfied"
        return "neutral"
    
    def _update_success_indicators(self, message: str, response: str, sentiment: str):
        """Track what works well for the user."""
        indicator = {
            "message_sample": message[:150],
            "response_sample": response[:150],
            "sentiment": sentiment,
            "timestamp": datetime.now().isoformat()
        }
        
        if sentiment == "satisfied":
            self.behavior_patterns["success_indicators"].append(indicator)
        elif sentiment == "frustrated":
            self.behavior_patterns["frustration_indicators"].append(indicator)
        
        # Keep recent
        max_each = 20
        for key in ["success_indicators", "frustration_indicators"]:
            if len(self.behavior_patterns[key]) > max_each:
                self.behavior_patterns[key] = self.behavior_patterns[key][-max_each:]
        
        self._save_behaviors()
    
    def _update_important_fact(self, key: str, value: str, confidence: float = 0.7):
        """Update an important fact about the user."""
        existing = self.profile["important_facts"].get(key)
        
        if existing:
            # Update with increased confidence if consistent
            if existing["value"] == value:
                confidence = min(1.0, existing["confidence"] + 0.1)
            else:
                confidence = existing["confidence"] * 0.5  # Reduce confidence on change
        else:
            confidence = 0.5  # New fact starts at 0.5
        
        self.profile["important_facts"][key] = {
            "value": value,
            "confidence": confidence,
            "last_updated": datetime.now().isoformat()
        }
        
        # Also store in vector DB for semantic retrieval
        self.vector_db.set_user_profile(key, value, confidence)
    
    def get_preferred_response_style(self) -> Dict:
        """Get the user's preferred response style for formatting replies."""
        style = self.profile["communication_style"]
        
        response_style = {
            "max_length": 500 if style["verbosity"] < 0.4 else 2000,
            "use_emoji": style["emoji_usage"] > 0.3,
            "formal": style["formality"] > 0.6,
            "include_bullets": style["verbosity"] > 0.5,
            "greeting": "Hello" if style["formality"] > 0.5 else "Hey"
        }
        
        return response_style
    
    def get_active_hours(self) -> List[int]:
        """Get the user's most active hours (for scheduling non-urgent tasks)."""
        if not self.profile["active_hours"]:
            return []
        
        # Sort by activity
        sorted_hours = sorted(
            self.profile["active_hours"].items(),
            key=lambda x: x[1],
            reverse=True
        )
        
        # Return top 4 most active hours
        return [h for h, _ in sorted_hours[:4]]
    
    def get_preferred_topics(self, limit: int = 5) -> List[str]:
        """Get the user's most discussed topics."""
        topic_counts = defaultdict(int)
        
        # Infer from topics_of_interest and tools_used
        for topic in self.profile["topics_of_interest"]:
            topic_counts[topic] += 1
        
        # Boost topics that appear in recent messages
        # (this would be enhanced with time-decay in production)
        
        sorted_topics = sorted(topic_counts.items(), key=lambda x: x[1], reverse=True)
        return [t for t, _ in sorted_topics[:limit]]
    
    def get_profile_summary(self) -> str:
        """Get a human-readable summary of the user profile."""
        style = self.get_preferred_response_style()
        
        summary_parts = [
            f"**Communication Style:** {'Formal' if style['formal'] else 'Casual'}, "
            f"{'verbose' if style['max_length'] > 1000 else 'concise'}",
        ]
        
        if self.profile.get("name"):
            summary_parts.append(f"**Name:** {self.profile['name']}")
        
        if self.profile.get("important_facts"):
            facts = self.profile["important_facts"]
            if "user_name" in facts:
                summary_parts.append(f"**Known as:** {facts['user_name']['value']}")
            if "current_project" in facts:
                summary_parts.append(f"**Working on:** {facts['current_project']['value']}")
            if "company" in facts:
                summary_parts.append(f"**Company:** {facts['company']['value']}")
        
        active_hours = self.get_active_hours()
        if active_hours:
            hour_str = ", ".join([f"{h}:00" for h in active_hours])
            summary_parts.append(f"**Active hours:** {hour_str}")
        
        topics = self.get_preferred_topics(3)
        if topics:
            summary_parts.append(f"**Topics:** {', '.join(topics)}")
        
        return " | ".join(summary_parts)
    
    def get_context_for_new_interaction(self) -> Dict[str, Any]:
        """
        Get all relevant context for a new interaction.
        Returns a dict meant to be injected into system context.
        """
        return {
            "user_profile_summary": self.get_profile_summary(),
            "preferred_response_style": self.get_preferred_response_style(),
            "active_hours": self.get_active_hours(),
            "preferred_topics": self.get_preferred_topics(5),
            "important_facts": {
                k: v["value"] 
                for k, v in self.profile["important_facts"].items()
                if v["confidence"] > 0.5
            },
            "success_patterns": [
                p["message_sample"] 
                for p in self.behavior_patterns.get("success_indicators", [])[-5:]
            ]
        }
    
    def get_stats(self) -> Dict:
        """Get user modeling statistics."""
        return {
            "total_topics_tracked": len(self.profile["topics_of_interest"]),
            "total_facts_tracked": len(self.profile["important_facts"]),
            "behavior_patterns": {
                k: len(v) for k, v in self.behavior_patterns.items()
            },
            "profile_confidence": {
                k: v["confidence"] 
                for k, v in self.profile.get("important_facts", {}).items()
            }
        }

