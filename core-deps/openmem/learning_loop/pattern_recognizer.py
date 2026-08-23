"""
Pattern Recognizer for OpenMem.
Identifies recurring patterns in conversation history using statistical analysis.
"""

import os
import json
import re
from datetime import datetime, timedelta
from typing import Dict, List, Any, Optional, Tuple
from collections import Counter, defaultdict
import math

from memory_store import get_vector_db
from memory_store.memory_manager import MemoryManager

# NOTE(dead code, pending Phase-2 wiring): PatternRecognizer.analyze_conversation_flow()
# and PatternRecognizer.update_patterns() currently have no production callers
# (only the test suite invokes them). They remain the sole writers of
# data/patterns.json and are intentionally kept until Phase-2 wires them into
# the learning loop.


class PatternRecognizer:
    """
    Recognizes patterns in conversation data.
    
    Identifies:
    - Recurring request types
    - Successful response strategies
    - User behavior patterns
    - Topic clusters
    - Temporal patterns (active times)
    """
    
    def __init__(self):
        self.vector_db = get_vector_db()
        self.memory_manager = MemoryManager()
        
        # Patterns storage
        self.patterns_file = os.path.join(
            os.path.dirname(__file__), "..", "data", "patterns.json"
        )
        os.makedirs(os.path.dirname(os.path.abspath(self.patterns_file)), exist_ok=True)
        self.patterns = self._load_patterns()
    
    def _load_patterns(self) -> Dict:
        """Load existing patterns."""
        if os.path.exists(self.patterns_file):
            with open(self.patterns_file, 'r') as f:
                return json.load(f)
        return {
            "request_types": {},    # request_pattern -> count
            "success_pairs": [],    # (request_type, successful_response_type)
            "topic_sequences": [],  # (topic_A, topic_B) transitions
            "temporal_patterns": {}, # hour -> request_types
            "last_updated": None
        }
    
    def _save_patterns(self):
        """Save patterns to disk."""
        self.patterns["last_updated"] = datetime.now().isoformat()
        with open(self.patterns_file, 'w') as f:
            json.dump(self.patterns, f, indent=2)
    
    # NOTE(dead code): no production callers yet — pending Phase-2 wiring
    # (see module-level note). Kept intentionally; writes nothing to disk here,
    # but its output feeds update_patterns() below.
    def analyze_conversation_flow(self, messages: List[Dict]) -> Dict:
        """
        Analyze the flow of a conversation to identify patterns.
        
        Args:
            messages: List of message dicts with 'role', 'content', 'timestamp'
            
        Returns:
            Analysis results with identified patterns
        """
        if not messages:
            return {}
        
        analysis = {
            "turn_count": len(messages),
            "request_types": [],
            "topics_mentioned": [],
            "strategy_used": None,
            "outcome": None
        }
        
        # Classify each user message
        user_messages = [m for m in messages if m.get("role") == "user"]
        assistant_messages = [m for m in messages if m.get("role") == "assistant"]
        
        for msg in user_messages:
            content = msg.get("content", "")
            request_type = self._classify_request(content)
            analysis["request_types"].append(request_type)
            
            topics = self._extract_topics(content)
            analysis["topics_mentioned"].extend(topics)
        
        # Identify strategy used by assistant
        if assistant_messages:
            strategy = self._classify_response_strategy(assistant_messages[-1].get("content", ""))
            analysis["strategy_used"] = strategy
        
        # Detect outcome
        if user_messages:
            last_user = user_messages[-1].get("content", "").lower()
            if any(kw in last_user for kw in ["thanks", "perfect", "great", "awesome"]):
                analysis["outcome"] = "success"
            elif any(kw in last_user for kw in ["still", "doesn't work", "wrong", "not good"]):
                analysis["outcome"] = "failure"
            else:
                analysis["outcome"] = "neutral"
        
        return analysis
    
    def _classify_request(self, content: str) -> str:
        """Classify a user request into a type."""
        content_lower = content.lower()

        # Comparison questions (check before general questions)
        if any(kw in content_lower for kw in ["difference", "differ from", "compare", " vs ", "versus", "vs.", "differs from"]):
            return "comparison_question"

        # Question patterns
        if content_lower.startswith(("what", "how", "why", "when", "where", "who", "which")):
            if any(kw in content_lower for kw in ["do you", "can you", "would you"]):
                return "capability_question"
            else:
                return "factual_question"
        
        # Action requests (order matters: problem_solving before build_request)
        if any(kw in content_lower for kw in ["fix", "debug", "solve", "bug"]):
            return "problem_solving"
        if any(kw in content_lower for kw in ["find", "search", "look up", "research"]):
            return "research_request"
        if any(kw in content_lower for kw in ["build", "create", "make", "write"]):
            return "build_request"
        if any(kw in content_lower for kw in ["explain", "tell me about", "what is"]):
            return "explanation_request"
        if any(kw in content_lower for kw in ["remember", "note", "save", "keep track"]):
            return "memory_request"
        if any(kw in content_lower for kw in ["help"]):
            return "problem_solving"
        
        # Meta requests
        if any(kw in content_lower for kw in ["change", "update", "modify", "improve"]):
            return "modification_request"
        
        return "general_conversation"
    
    def _classify_response_strategy(self, content: str) -> str:
        """Classify an assistant response strategy."""
        content_lower = content.lower()

        if "```" in content:
            return "code_oriented"
        elif "##" in content:
            return "structured_format"
        elif any(kw in content_lower for kw in ["first", "second", "third", "finally", "in conclusion"]):
            return "step_by_step"
        elif len(content) < 100:
            return "concise_direct"
        elif len(content) > 1000:
            return "detailed_explanatory"

        return "conversational"
    
    def _extract_topics(self, content: str) -> List[str]:
        """Extract topics from content."""
        topics = []
        content_lower = content.lower()
        
        topic_signatures = {
            "python": ["python", "pip", "venv", "pypi"],
            "javascript": ["javascript", "js", "node", "npm", "nodejs"],
            "web_dev": ["html", "css", "react", "vue", "frontend", "backend"],
            "databases": ["sql", "mysql", "postgresql", "mongodb", "redis", "database"],
            "devops": ["docker", "kubernetes", "ci/cd", "deploy", "server", "aws", "cloud"],
            "ai_ml": ["ai", "ml", "machine learning", "model", "gpt", "llm", "neural"],
            "openclaw": ["openclaw", "skill", "gateway", "channel", "messaging"],
            "coding": ["code", "function", "class", "api", "debug", "bug"],
            "project": ["project", "building", "creating", "implementing"],
        }
        
        for topic, keywords in topic_signatures.items():
            if any(kw in content_lower for kw in keywords):
                topics.append(topic)
        
        return topics
    
    def find_recurring_patterns(self, days_back: int = 7) -> List[Dict]:
        """
        Find recurring patterns in recent memories.
        
        Returns list of pattern dicts with:
        - type: pattern type
        - pattern: the actual pattern
        - frequency: how often it occurs
        - confidence: how certain we are
        """
        from datetime import timedelta
        
        cutoff = (datetime.now() - timedelta(days=days_back)).isoformat()
        
        # Get recent memories from vector DB
        recent = self.vector_db.get_recent_memories(hours=days_back * 24, limit=500)
        
        patterns = {
            "request_type_counter": Counter(),
            "topic_counter": Counter(),
            "topic_transitions": Counter(),
            "successful_sequences": []
        }
        
        last_topic = None
        last_request = None
        
        for memory in recent:
            content = memory.get("content", "")

            # Request-type classification applies ONLY to user messages.
            # Classifying assistant/system memories here inflated request-type
            # counts (role is stored in metadata by ConversationIndexer).
            role = memory.get("role") or memory.get("metadata", {}).get("role")

            request_type = None
            if role == "user":
                request_type = self._classify_request(content)
                patterns["request_type_counter"][request_type] += 1

            topics = self._extract_topics(content)

            for topic in topics:
                patterns["topic_counter"][topic] += 1

                # Track transitions
                if last_topic:
                    patterns["topic_transitions"][(last_topic, topic)] += 1
                last_topic = topic

            # Success indicators
            if any(kw in content.lower() for kw in ["perfect", "thanks", "great"]):
                if last_request:
                    patterns["successful_sequences"].append({
                        "request": last_request,
                        "topic": topics[0] if topics else "general",
                        "timestamp": memory.get("timestamp")
                    })

            if request_type:
                last_request = request_type
        
        # Build pattern results
        results = []
        
        # High-frequency request types
        for req_type, count in patterns["request_type_counter"].most_common(5):
            if count >= 3:
                results.append({
                    "type": "request_type",
                    "pattern": req_type,
                    "frequency": count,
                    "confidence": min(1.0, count / 10),
                    "recommendation": f"Consider creating a skill for frequent request type: {req_type}"
                })
        
        # High-frequency topics
        for topic, count in patterns["topic_counter"].most_common(5):
            if count >= 5:
                results.append({
                    "type": "topic",
                    "pattern": topic,
                    "frequency": count,
                    "confidence": min(1.0, count / 15),
                    "recommendation": f"Build expertise/resources around: {topic}"
                })
        
        # Common topic transitions
        for (from_topic, to_topic), count in patterns["topic_transitions"].most_common(5):
            if count >= 3:
                results.append({
                    "type": "topic_transition",
                    "pattern": f"{from_topic} → {to_topic}",
                    "frequency": count,
                    "confidence": min(1.0, count / 8),
                    "recommendation": f"Expect users to ask about {to_topic} after {from_topic}"
                })
        
        return results
    
    # NOTE(dead code): no production callers yet — pending Phase-2 wiring
    # (see module-level note). Sole writer of data/patterns.json.
    def update_patterns(self, conversation_analysis: Dict):
        """Update stored patterns with new conversation analysis."""
        if not conversation_analysis.get("request_types"):
            return
        
        for request_type in conversation_analysis["request_types"]:
            self.patterns["request_types"][request_type] = \
                self.patterns["request_types"].get(request_type, 0) + 1
        
        topics = conversation_analysis.get("topics_mentioned", [])
        for i in range(len(topics) - 1):
            transition = (topics[i], topics[i + 1])
            self.patterns["topic_sequences"].append(transition)
        
        if conversation_analysis.get("outcome") == "success":
            for req_type in conversation_analysis["request_types"]:
                self.patterns["success_pairs"].append({
                    "request": req_type,
                    "strategy": conversation_analysis.get("strategy_used"),
                    "timestamp": datetime.now().isoformat()
                })
        
        # Keep only recent
        max_pairs = 1000
        if len(self.patterns["success_pairs"]) > max_pairs:
            self.patterns["success_pairs"] = self.patterns["success_pairs"][-max_pairs:]
        
        self._save_patterns()
    
    def get_recommended_skills(self) -> List[Dict]:
        """
        Get skill recommendations based on discovered patterns.
        """
        patterns = self.find_recurring_patterns(days_back=7)
        
        recommendations = []
        for pattern in patterns:
            if pattern.get("frequency", 0) >= 3:
                recommendations.append({
                    "trigger": pattern["pattern"],
                    "type": pattern["type"],
                    "frequency": pattern["frequency"],
                    "confidence": pattern["confidence"],
                    "action": pattern.get("recommendation")
                })
        
        # Sort by confidence * frequency
        recommendations.sort(key=lambda x: x["confidence"] * x["frequency"], reverse=True)
        
        return recommendations[:10]
    
    def get_stats(self) -> Dict:
        """Get pattern recognition statistics."""
        return {
            "total_patterns": sum(len(v) if isinstance(v, (list, dict)) else 1 for v in self.patterns.values()),
            "request_type_counts": dict(self.patterns.get("request_types", {})),
            "top_topics": dict(Counter(
                t for seq in self.patterns.get("topic_sequences", []) 
                for t in seq if isinstance(seq, tuple)
            ).most_common(10)),
            "success_pairs_count": len(self.patterns.get("success_pairs", [])),
            "last_updated": self.patterns.get("last_updated")
        }
