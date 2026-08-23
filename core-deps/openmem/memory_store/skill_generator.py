"""
Skill Generator for OpenMem.
Automatically generates OpenClaw skills from recurring successful interaction patterns.
"""

import os
import json
import re
import shutil
from datetime import datetime
from typing import Dict, List, Any, Optional, Tuple
from pathlib import Path

from . import get_vector_db
from .memory_manager import MemoryManager


# Template for OpenClaw SKILL.md
SKILL_TEMPLATE = '''# {name}

_Auto-generated skill from OpenMem learning loop._

{description}

## Triggers

This skill activates when:
{triggers}

## Actions

{actions}

## Examples

{examples}

## Notes

- Auto-generated: {generated_date}
- Pattern confidence: {confidence:.0%}
- Times used successfully: {usage_count}
'''


class SkillGenerator:
    """
    Generates OpenClaw skills from patterns discovered in conversation history.
    
    Process:
    1. Scan conversation history for recurring successful patterns
    2. Identify trigger conditions (keywords, contexts, user requests)
    3. Identify successful response patterns (what worked well)
    4. Generate SKILL.md and learner.py files
    """
    
    def __init__(self, skills_output_path: str = None, openclaw_skills_dir: str = None):
        self.vector_db = get_vector_db()
        self.memory_manager = MemoryManager()

        # Output path for generated skills
        base = os.path.dirname(__file__)
        self.skills_output_path = skills_output_path or os.path.join(base, "..", "generated_skills")
        self.skills_output_path = os.path.abspath(self.skills_output_path)
        os.makedirs(self.skills_output_path, exist_ok=True)

        # OpenClaw skills directory (for reference/integration)
        self.openclaw_skills_dir = openclaw_skills_dir

        # Skill registry in centralized data/ directory
        self.registry_path = os.path.join(
            os.path.dirname(__file__), "..", "data", "skill_registry.json"
        )
        os.makedirs(os.path.dirname(os.path.abspath(self.registry_path)), exist_ok=True)
        self.registry = self._load_registry()
    
    def _load_registry(self) -> Dict:
        """Load existing skill registry."""
        if os.path.exists(self.registry_path):
            with open(self.registry_path, 'r') as f:
                return json.load(f)
        return {
            "skills": {},  # skill_name -> {path, triggers, confidence, generated_at, usage_count}
            "patterns_discovered": [],
            "last_scan": None
        }
    
    def _save_registry(self):
        """Save skill registry."""
        with open(self.registry_path, 'w') as f:
            json.dump(self.registry, f, indent=2)
    
    def discover_patterns(self, hours_back: int = 168) -> List[Dict]:
        """
        Scan recent conversation history for recurring patterns.
        Returns list of discovered patterns.
        """
        recent_memories = self.vector_db.get_recent_memories(hours=hours_back)
        
        patterns = {
            "keyword_clusters": {},  # keyword -> frequency
            "context_sequences": [],  # (trigger, action) pairs
            "successful_routes": []   # User request -> Successful response pattern
        }
        
        for memory in recent_memories:
            content = memory["content"].lower()
            
            # Find keyword clusters (words that appear together)
            words = content.split()
            significant_words = [w for w in words if len(w) > 4 and w not in [
                "there", "would", "could", "should", "which", "where", "their", "these"
            ]]
            
            for word in significant_words:
                patterns["keyword_clusters"][word] = patterns["keyword_clusters"].get(word, 0) + 1
            
            # Look for success indicators
            success_phrases = [
                "perfect", "thanks", "great", "awesome", "works", "nice", "good",
                "that worked", "exactly", "love it"
            ]
            
            if any(phrase in content for phrase in success_phrases):
                patterns["successful_routes"].append({
                    "content": memory["content"][:200],
                    "timestamp": memory["timestamp"],
                    "importance": memory["importance"]
                })
        
        # Find high-frequency clusters (potential skill triggers)
        high_freq_keywords = [
            kw for kw, count in patterns["keyword_clusters"].items()
            if count >= 3
        ]
        
        patterns["high_freq_keywords"] = high_freq_keywords
        
        self.registry["patterns_discovered"] = patterns
        self.registry["last_scan"] = datetime.now().isoformat()
        self._save_registry()
        
        return patterns
    
    def generate_skill_from_pattern(self, pattern: Dict, skill_name: str = None) -> Optional[Dict]:
        """
        Generate a skill file from a discovered pattern.
        Returns skill metadata if successful.
        """
        if not pattern.get("high_freq_keywords"):
            return None
        
        # Determine skill name from dominant keywords
        if not skill_name:
            top_keywords = pattern["high_freq_keywords"][:3]
            skill_name = "_".join(top_keywords)
        
        # Clean skill name
        skill_name = re.sub(r'[^a-zA-Z0-9_-]', '', skill_name)
        skill_name = f"auto_{skill_name[:30]}"
        
        # Generate triggers from keywords
        triggers = []
        for kw in pattern["high_freq_keywords"][:5]:
            triggers.append(f"- User mentions '{kw}' (appears {pattern['keyword_clusters'].get(kw, 0)}x)")
        
        # Generate actions based on successful routes
        actions = []
        if pattern.get("successful_routes"):
            actions.append("- Reference successful responses from past interactions")
            actions.append("- Apply the same approach that worked previously")
        
        actions.append("- Use memory search to find relevant context")
        actions.append("- Provide helpful, actionable response")
        
        # Generate examples from successful routes
        examples = []
        for route in pattern["successful_routes"][:3]:
            examples.append(f"```\n{route['content']}\n```")
        
        if not examples:
            examples.append("```\nUser: [pattern trigger]\nAssistant: [helpful response]\n```")
        
        # Build skill content
        skill_content = SKILL_TEMPLATE.format(
            name=skill_name.replace("_", " ").title(),
            description=f"Auto-generated skill triggered by recurring pattern: {', '.join(pattern['high_freq_keywords'][:3])}",
            triggers="\n".join(triggers) if triggers else "- User requests related to this topic",
            actions="\n".join(actions),
            examples="\n\n".join(examples),
            generated_date=datetime.now().strftime("%Y-%m-%d"),
            confidence=0.7,
            usage_count=0
        )
        
        # Create skill directory
        skill_dir = os.path.join(self.skills_output_path, skill_name)
        os.makedirs(skill_dir, exist_ok=True)
        
        # Write SKILL.md
        skill_md_path = os.path.join(skill_dir, "SKILL.md")
        with open(skill_md_path, 'w') as f:
            f.write(skill_content)
        
        # Write learner.py (the execution logic)
        learner_content = self._generate_learner_code(skill_name, pattern)
        learner_path = os.path.join(skill_dir, "learner.py")
        with open(learner_path, 'w') as f:
            f.write(learner_content)
        
        # Write metadata
        metadata = {
            "name": skill_name,
            "path": skill_dir,
            "triggers": pattern["high_freq_keywords"][:5],
            "confidence": 0.7,
            "generated_at": datetime.now().isoformat(),
            "usage_count": 0,
            "pattern": {
                "keywords": pattern["high_freq_keywords"],
                "keyword_counts": {k: pattern["keyword_clusters"].get(k, 0) for k in pattern["high_freq_keywords"]}
            }
        }
        
        metadata_path = os.path.join(skill_dir, "metadata.json")
        with open(metadata_path, 'w') as f:
            json.dump(metadata, f, indent=2)
        
        # Update registry
        self.registry["skills"][skill_name] = metadata
        self._save_registry()
        
        return metadata
    
    def _generate_learner_code(self, skill_name: str, pattern: Dict) -> str:
        """Generate the Python learner code for a skill."""
        keywords = pattern.get("high_freq_keywords", [])[:5]
        keyword_list = ", ".join([f"'{kw}'" for kw in keywords])
        
        return f'''"""
Learner module for skill: {skill_name}
Auto-generated by OpenMem Skill Generator.

This skill activates when user mentions: {keyword_list}
"""

import re
from typing import Dict, Any, List


def should_activate(context: Dict[str, Any]) -> bool:
    """
    Determine if this skill should activate based on context.
    Returns True if the user's message matches our trigger patterns.
    """
    user_message = context.get("message", "").lower()
    
    # Trigger keywords for this skill
    triggers = [{keyword_list}]
    
    # Check if any trigger keyword is in the message
    for trigger in triggers:
        if trigger in user_message:
            return True
    
    return False


def execute(context: Dict[str, Any]) -> Dict[str, Any]:
    """
    Execute the skill logic.
    
    Args:
        context: Dict containing:
            - message: user's message
            - memory_manager: MemoryManager instance for context
            - user_model: UserModel instance
            
    Returns:
        Dict with:
            - response: text response to user
            - actions: list of actions to take
            - memory_updates: list of memories to store
    """
    from memory_store import get_vector_db, MemoryManager
    
    message = context.get("message", "")
    memory_manager = context.get("memory_manager") or MemoryManager()
    user_model = context.get("user_model")
    
    response_parts = []
    actions = []
    memory_updates = []
    
    # Search relevant memory context
    query = message
    relevant_memories = memory_manager.search_memory(query, n_results=3)
    
    if relevant_memories:
        response_parts.append("Based on our previous interactions:\\n")
        for mem in relevant_memories[:2]:
            response_parts.append(f"- {{mem['content'][:150]}}")
    
    # Get user preferences if available
    if user_model:
        pref_style = user_model.get_preferred_response_style()
        response_parts.append(f"\\n(Detected style: {{'formal' if pref_style.get('formal') else 'casual'}})")
    
    response = "\\n".join(response_parts) if response_parts else None
    
    # Record this interaction for learning
    memory_updates.append({{
        "content": f"Skill {skill_name} activated for: {{message[:100]}}",
        "importance": 0.6,
        "tags": ["skill_use", "{skill_name}"],
        "metadata": {{"skill": "{skill_name}"}}
    }})
    
    return {{
        "response": response,
        "actions": actions,
        "memory_updates": memory_updates,
        "skill_name": "{skill_name}"
    }}


def get_metadata() -> Dict[str, Any]:
    """Return skill metadata."""
    return {{
        "name": "{skill_name}",
        "type": "auto_generated",
        "version": "1.0.0",
        "generated": "{datetime.now().isoformat()}",
        "triggers": [{keyword_list}]
    }}
'''
    
    def generate_all_skills_from_patterns(self, min_frequency: int = 3) -> List[Dict]:
        """
        Scan for patterns and generate skills for all qualifying ones.
        
        Args:
            min_frequency: Minimum keyword frequency to trigger skill generation
            
        Returns:
            List of generated skill metadata
        """
        patterns = self.discover_patterns()
        
        if not patterns.get("high_freq_keywords"):
            print("[SkillGenerator] No qualifying patterns found")
            return []
        
        # Filter for high-frequency keywords
        qualifying_keywords = [
            kw for kw, count in patterns["keyword_clusters"].items()
            if count >= min_frequency
        ]
        
        if not qualifying_keywords:
            return []
        
        # Create pattern dict for skill generation
        pattern_for_skill = {
            "high_freq_keywords": qualifying_keywords[:5],
            "keyword_clusters": patterns["keyword_clusters"],
            "successful_routes": patterns.get("successful_routes", [])
        }
        
        skill_meta = self.generate_skill_from_pattern(pattern_for_skill)
        
        return [skill_meta] if skill_meta else []
    
    def get_generated_skills(self) -> List[Dict]:
        """Get list of all generated skills."""
        return list(self.registry.get("skills", {}).values())
    
    def increment_usage(self, skill_name: str):
        """Increment usage counter for a skill."""
        if skill_name in self.registry.get("skills", {}):
            self.registry["skills"][skill_name]["usage_count"] += 1
            self._save_registry()
    
    def get_skill_by_name(self, name: str) -> Optional[Dict]:
        """Get a specific skill's metadata."""
        return self.registry.get("skills", {}).get(name)
    
    def get_stats(self) -> Dict:
        """Get skill generation statistics."""
        skills = self.registry.get("skills", {})
        
        total_usage = sum(s.get("usage_count", 0) for s in skills.values())
        
        return {
            "total_skills_generated": len(skills),
            "total_uses": total_usage,
            "skills": [
                {
                    "name": s["name"],
                    "usage_count": s.get("usage_count", 0),
                    "confidence": s.get("confidence", 0)
                }
                for s in skills.values()
            ],
            "last_pattern_scan": self.registry.get("last_scan")
        }
    
    def export_to_openclaw_format(self, skill_name: str, output_dir: str = None) -> Optional[str]:
        """
        Export a generated skill to OpenClaw skills directory format.
        Returns the path where files were written.
        """
        skill = self.get_skill_by_name(skill_name)
        if not skill:
            return None
        
        if not output_dir:
            output_dir = self.openclaw_skills_dir
        
        if not output_dir or not os.path.exists(output_dir):
            print(f"[SkillGenerator] OpenClaw skills dir not found: {output_dir}")
            return None
        
        skill_dir = os.path.join(output_dir, skill_name)
        os.makedirs(skill_dir, exist_ok=True)
        
        source_dir = skill["path"]
        
        # Copy SKILL.md
        src_skill_md = os.path.join(source_dir, "SKILL.md")
        if os.path.exists(src_skill_md):
            shutil.copy2(src_skill_md, os.path.join(skill_dir, "SKILL.md"))
        
        # Copy learner.py
        src_learner = os.path.join(source_dir, "learner.py")
        if os.path.exists(src_learner):
            shutil.copy2(src_learner, os.path.join(skill_dir, "learner.py"))
        
        # Copy metadata
        src_meta = os.path.join(source_dir, "metadata.json")
        if os.path.exists(src_meta):
            shutil.copy2(src_meta, os.path.join(skill_dir, "metadata.json"))
        
        print(f"[SkillGenerator] Exported {skill_name} to {skill_dir}")
        return skill_dir
