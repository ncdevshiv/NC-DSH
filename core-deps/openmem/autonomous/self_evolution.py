"""
Self-Evolution Engine for OpenMem.
Genetic algorithm-based optimization of skills and strategies.
"""

import os
import json
import random
import copy
import logging
from typing import Dict, List, Any, Optional, Callable
from datetime import datetime, timedelta
from collections import defaultdict
import hashlib

logger = logging.getLogger("openmem.evolution")


class EvolutionEngine:
    """
    Self-evolution system using genetic algorithms.
    
    Concepts:
    - Population = skills, strategies, response patterns
    - Fitness = performance score from MatrixPruner
    - Selection = keeping top performers
    - Crossover = combining two strategies to create new hybrid
    - Mutation = random changes to create diversity
    
    The system automatically evolves:
    1. Skills (improve their triggers/actions)
    2. Strategies (improve how we approach problems)
    3. Response patterns (improve how we respond)
    """
    
    def __init__(self, base_path: str = None):
        self.base_path = base_path or os.path.join(os.path.dirname(__file__), "..", "data", "evolution")
        os.makedirs(self.base_path, exist_ok=True)
        
        # Evolution config
        self.population_size = 20  # Max items in population
        self.mutation_rate = 0.15  # 15% chance of mutation
        self.crossover_rate = 0.3  # 30% chance of crossover
        self.elite_ratio = 0.2  # Keep top 20% as-is
        self.generation_file = os.path.join(self.base_path, "generation_state.json")
        
        # Load or initialize generation state
        self.state = self._load_state()
        
        # Gene definitions
        self.gene_definitions = self._init_gene_definitions()
    
    def _load_state(self) -> Dict:
        """Load evolution state."""
        if os.path.exists(self.generation_file):
            with open(self.generation_file, 'r') as f:
                return json.load(f)
        
        return {
            "generation": 0,
            "population": [],  # List of evolving entities
            "hall_of_fame": [],  # Best of all time
            "evolution_log": [],
            "last_evolution": None,
            "convergence_count": 0  # Times evolution stopped improving
        }
    
    def _save_state(self):
        """Save evolution state."""
        self.state["last_evolution"] = datetime.now().isoformat()
        with open(self.generation_file, 'w') as f:
            json.dump(self.state, f, indent=2)
    
    def _init_gene_definitions(self) -> Dict:
        """Define gene structure for different entity types."""
        return {
            "skill": {
                "genes": ["trigger_keywords", "action_style", "response_length", "formality", "examples_provided"],
                "ranges": {
                    "response_length": (50, 2000),
                    "formality": (0.0, 1.0),
                    "examples_provided": (0, 5)
                },
                "mutations": {
                    "trigger_keywords": lambda x: self._mutate_keywords(x),
                    "action_style": lambda x: random.choice(["concise", "detailed", "step_by_step", "conversational"]),
                    "response_length": lambda x: max(50, min(2000, x + random.gauss(0, 200))),
                    "formality": lambda x: max(0.0, min(1.0, x + random.gauss(0, 0.1))),
                    "examples_provided": lambda x: max(0, min(5, x + random.choice([-1, 0, 1])))
                }
            },
            "strategy": {
                "genes": ["approach_type", "depth", "creativity", "risk_tolerance", "adaptability"],
                "ranges": {
                    "depth": (1, 5),
                    "creativity": (0.0, 1.0),
                    "risk_tolerance": (0.0, 1.0),
                    "adaptability": (0.0, 1.0)
                },
                "mutations": {
                    "approach_type": lambda x: random.choice(["direct", "exploratory", "collaborative", "systematic"]),
                    "depth": lambda x: max(1, min(5, x + random.choice([-1, 0, 1]))),
                    "creativity": lambda x: max(0.0, min(1.0, x + random.gauss(0, 0.15))),
                    "risk_tolerance": lambda x: max(0.0, min(1.0, x + random.gauss(0, 0.1))),
                    "adaptability": lambda x: max(0.0, min(1.0, x + random.gauss(0, 0.1)))
                }
            }
        }
    
    def _mutate_keywords(self, keywords: List[str]) -> List[str]:
        """Mutate keyword list."""
        mutations = ["add", "remove", "replace", "swap"]
        choice = random.choice(mutations)
        
        new_keywords = list(keywords)
        
        if choice == "add" and len(new_keywords) < 10:
            new_keywords.append(f"topic_{random.randint(100, 999)}")
        elif choice == "remove" and len(new_keywords) > 1:
            new_keywords.remove(random.choice(new_keywords))
        elif choice == "replace" and new_keywords:
            idx = random.randint(0, len(new_keywords) - 1)
            new_keywords[idx] = f"new_keyword_{random.randint(100, 999)}"
        elif choice == "swap" and len(new_keywords) >= 2:
            i, j = random.sample(range(len(new_keywords)), 2)
            new_keywords[i], new_keywords[j] = new_keywords[j], new_keywords[i]
        
        return new_keywords
    
    def create_initial_population(self, skills: List[Dict] = None, strategies: List[Dict] = None):
        """Initialize population from existing skills/strategies."""
        population = []
        
        # Add existing skills
        if skills:
            for skill in skills:
                entity = {
                    "id": f"skill_{skill.get('name', 'unknown')}_{len(population)}",
                    "type": "skill",
                    "genes": {
                        "trigger_keywords": skill.get("triggers", [])[:5],
                        "action_style": "detailed",
                        "response_length": 500,
                        "formality": 0.5,
                        "examples_provided": 2
                    },
                    "fitness": skill.get("usage_count", 0) * skill.get("confidence", 0.5),
                    "created_at": datetime.now().isoformat(),
                    "parent_ids": []
                }
                population.append(entity)
        
        # Add seed strategies
        if strategies:
            for strategy in strategies:
                entity = {
                    "id": f"strategy_{strategy.get('name', 'unknown')}_{len(population)}",
                    "type": "strategy",
                    "genes": {
                        "approach_type": "direct",
                        "depth": 3,
                        "creativity": 0.5,
                        "risk_tolerance": 0.3,
                        "adaptability": 0.6
                    },
                    "fitness": strategy.get("fitness", 0.5),
                    "created_at": datetime.now().isoformat(),
                    "parent_ids": []
                }
                population.append(entity)
        
        # Fill remaining slots with random entities
        while len(population) < self.population_size:
            entity = self._create_random_entity("skill")
            population.append(entity)
        
        self.state["population"] = population
        self._save_state()
    
    def _create_random_entity(self, entity_type: str) -> Dict:
        """Create a random entity."""
        gene_def = self.gene_definitions.get(entity_type, {})
        
        genes = {}
        for gene_name, gene_range in gene_def.get("ranges", {}).items():
            if isinstance(gene_range, tuple) and isinstance(gene_range[0], float):
                genes[gene_name] = random.uniform(gene_range[0], gene_range[1])
            else:
                genes[gene_name] = random.randint(gene_range[0], gene_range[1])
        
        # Non-range genes
        if entity_type == "skill":
            genes["trigger_keywords"] = [f"keyword_{i}" for i in range(random.randint(2, 5))]
            genes["action_style"] = random.choice(["concise", "detailed", "step_by_step"])
        
        return {
            "id": f"{entity_type}_{datetime.now().strftime('%H%M%S')}_{random.randint(1000, 9999)}",
            "type": entity_type,
            "genes": genes,
            "fitness": 0.5,
            "created_at": datetime.now().isoformat(),
            "parent_ids": []
        }
    
    def update_fitness(self, entity_id: str, new_fitness: float):
        """Update fitness score for an entity."""
        for entity in self.state["population"]:
            if entity["id"] == entity_id:
                old_fitness = entity["fitness"]
                entity["fitness"] = new_fitness
                
                # Log
                self.state["evolution_log"].append({
                    "type": "fitness_update",
                    "entity_id": entity_id,
                    "old_fitness": old_fitness,
                    "new_fitness": new_fitness,
                    "timestamp": datetime.now().isoformat()
                })
                break
        
        # Update hall of fame
        self._update_hall_of_fame()
        self._save_state()
    
    def _update_hall_of_fame(self):
        """Maintain hall of fame with best entities."""
        sorted_pop = sorted(self.state["population"], key=lambda x: x["fitness"], reverse=True)
        
        current_hof = {e["id"] for e in self.state["hall_of_fame"]}
        
        for entity in sorted_pop[:5]:
            if entity["id"] not in current_hof:
                hall_entity = copy.deepcopy(entity)
                hall_entity["added_to_hof"] = datetime.now().isoformat()
                self.state["hall_of_fame"].append(hall_entity)
        
        # Keep only top 20 in hall of fame
        self.state["hall_of_fame"] = sorted(
            self.state["hall_of_fame"], 
            key=lambda x: x["fitness"], 
            reverse=True
        )[:20]
    
    def selection(self) -> List[Dict]:
        """Select entities for next generation using tournament selection."""
        sorted_pop = sorted(self.state["population"], key=lambda x: x["fitness"], reverse=True)
        
        # Elitism: keep top performers
        elite_count = int(len(sorted_pop) * self.elite_ratio)
        elites = sorted_pop[:elite_count]
        
        return elites
    
    def crossover(self, parent1: Dict, parent2: Dict) -> Optional[Dict]:
        """Crossover two entities to create offspring."""
        if random.random() > self.crossover_rate:
            return None
        
        if parent1["type"] != parent2["type"]:
            return None
        
        child = copy.deepcopy(parent1)
        child["id"] = f"{parent1['type']}_child_{datetime.now().strftime('%H%M%S')}_{random.randint(1000, 9999)}"
        child["parent_ids"] = [parent1["id"], parent2["id"]]
        child["created_at"] = datetime.now().isoformat()
        child["fitness"] = (parent1["fitness"] + parent2["fitness"]) / 2
        
        # Swap genes
        for gene_name in child["genes"]:
            if random.random() < 0.5:
                child["genes"][gene_name] = parent2["genes"][gene_name]
        
        return child
    
    def mutate(self, entity: Dict) -> Dict:
        """Mutate an entity's genes."""
        if random.random() > self.mutation_rate:
            return entity
        
        gene_def = self.gene_definitions.get(entity["type"], {})
        mutations = gene_def.get("mutations", {})
        
        for gene_name, mutation_fn in mutations.items():
            if gene_name in entity["genes"] and random.random() < 0.3:
                try:
                    entity["genes"][gene_name] = mutation_fn(entity["genes"][gene_name])
                except Exception as e:
                    logger.debug(f"Mutation failed for gene '{gene_name}': {e}")
        
        entity["last_mutation"] = datetime.now().isoformat()
        return entity
    
    def evolve(self) -> Dict:
        """
        Run one evolution cycle.
        Returns evolution report.
        """
        report = {
            "generation": self.state["generation"],
            "started_at": datetime.now().isoformat(),
            "selected": 0,
            "crossovers": 0,
            "mutations": 0,
            "new_entities": 0,
            "population_size": len(self.state["population"]),
            "avg_fitness": 0,
            "best_fitness": 0,
            "best_entity_id": None,
            "converged": False
        }
        
        if not self.state["population"]:
            self.create_initial_population()
        
        # Calculate fitness stats
        fitnesses = [e["fitness"] for e in self.state["population"]]
        report["avg_fitness"] = sum(fitnesses) / len(fitnesses) if fitnesses else 0
        report["best_fitness"] = max(fitnesses) if fitnesses else 0
        
        best_entity = max(self.state["population"], key=lambda x: x["fitness"], default=None)
        if best_entity:
            report["best_entity_id"] = best_entity["id"]
        
        # Check for convergence
        if report["best_fitness"] > 0.9:
            self.state["convergence_count"] += 1
            if self.state["convergence_count"] > 3:
                report["converged"] = True
                self.state["convergence_count"] = 0
        else:
            self.state["convergence_count"] = 0
        
        # Selection
        elites = self.selection()
        report["selected"] = len(elites)
        
        # Create new generation
        new_population = list(elites)  # Start with elites
        
        # Fill remaining slots
        while len(new_population) < self.population_size:
            # Try crossover
            if len(elites) >= 2:
                parent1, parent2 = random.sample(elites, 2)
                child = self.crossover(parent1, parent2)
                if child:
                    child = self.mutate(child)
                    new_population.append(child)
                    report["crossovers"] += 1
                    if "last_mutation" in child:
                        report["mutations"] += 1
                    continue
            
            # Create random entity
            entity_type = random.choice(["skill", "strategy"])
            new_entity = self._create_random_entity(entity_type)
            new_entity = self.mutate(new_entity)
            new_population.append(new_entity)
            report["new_entities"] += 1
        
        # Update state
        self.state["generation"] += 1
        self.state["population"] = new_population
        
        # Log
        self.state["evolution_log"].append({
            "type": "evolution",
            "generation": self.state["generation"],
            "avg_fitness": report["avg_fitness"],
            "best_fitness": report["best_fitness"],
            "timestamp": datetime.now().isoformat()
        })
        
        # Keep log size manageable
        if len(self.state["evolution_log"]) > 1000:
            self.state["evolution_log"] = self.state["evolution_log"][-500:]
        
        report["completed_at"] = datetime.now().isoformat()
        self._save_state()
        
        return report
    
    def get_best_strategy(self) -> Optional[Dict]:
        """Get the best evolved strategy."""
        if not self.state["population"]:
            return None
        
        best = max(self.state["population"], key=lambda x: x["fitness"])
        return best if best["type"] == "strategy" else None
    
    def get_best_skills(self, limit: int = 5) -> List[Dict]:
        """Get top performing skill entities."""
        skills = [e for e in self.state["population"] if e["type"] == "skill"]
        return sorted(skills, key=lambda x: x["fitness"], reverse=True)[:limit]
    
    def get_entity_by_id(self, entity_id: str) -> Optional[Dict]:
        """Get a specific entity."""
        for entity in self.state["population"]:
            if entity["id"] == entity_id:
                return entity
        for entity in self.state["hall_of_fame"]:
            if entity["id"] == entity_id:
                return entity
        return None
    
    def apply_evolved_skill(self, entity_id: str) -> Optional[Dict]:
        """
        Apply an evolved skill back to OpenMem skill generator.
        Returns the skill dict if successful.
        """
        entity = self.get_entity_by_id(entity_id)
        if not entity or entity["type"] != "skill":
            return None
        
        # Convert evolved genes to OpenMem skill format
        skill = {
            "name": entity["genes"].get("trigger_keywords", ["untitled"])[0],
            "triggers": entity["genes"].get("trigger_keywords", []),
            "action_style": entity["genes"].get("action_style", "detailed"),
            "response_length": int(entity["genes"].get("response_length", 500)),
            "formality": entity["genes"].get("formality", 0.5),
            "examples_provided": int(entity["genes"].get("examples_provided", 2)),
            "fitness": entity["fitness"],
            "generation": self.state["generation"],
            "parent_id": entity_id
        }
        
        return skill
    
    def force_evolution(self) -> Dict:
        """Force a new generation even if converged."""
        # Add fresh random entities
        for _ in range(5):
            entity_type = random.choice(["skill", "strategy"])
            self.state["population"].append(self._create_random_entity(entity_type))
        
        self.state["convergence_count"] = 0
        return self.evolve()
    
    def get_stats(self) -> Dict:
        """Get evolution statistics."""
        fitnesses = [e["fitness"] for e in self.state["population"]]
        
        type_counts = defaultdict(int)
        for e in self.state["population"]:
            type_counts[e["type"]] += 1
        
        return {
            "generation": self.state["generation"],
            "population_size": len(self.state["population"]),
            "type_distribution": dict(type_counts),
            "avg_fitness": sum(fitnesses) / len(fitnesses) if fitnesses else 0,
            "best_fitness": max(fitnesses) if fitnesses else 0,
            "worst_fitness": min(fitnesses) if fitnesses else 0,
            "hall_of_fame_size": len(self.state["hall_of_fame"]),
            "convergence_count": self.state["convergence_count"],
            "last_evolution": self.state.get("last_evolution")
        }
