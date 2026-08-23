"""
Autonomous package for LanceMem.
Self-optimization and self-evolution powered by LanceDB.
"""

from .self_optimizer import LanceDBOptimizer, MatrixPruner, get_optimizer
from .self_evolution import EvolutionEngine

__all__ = [
    "LanceDBOptimizer",
    "MatrixPruner",
    "get_optimizer",
    "EvolutionEngine"
]
