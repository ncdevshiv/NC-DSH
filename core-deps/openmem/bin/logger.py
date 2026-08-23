"""
OpenMem — Centralized Logger.

Single logging utility used by all modules. No bare except:pass anywhere.

Usage:
    from bin.logger import get_logger

    log = get_logger("vector_db")
    log.info("Connected to database")
    log.error("Failed to add memory", exc_info=True)
    log.debug("Query processed in 0.5ms")
"""

import os
import sys
import logging
from pathlib import Path
from datetime import datetime

LOG_DIR = Path(__file__).parent.parent / "data" / "logs"
LOG_FILE = LOG_DIR / "openmem.log"

# Ensure log directory exists
os.makedirs(LOG_DIR, exist_ok=True)

# Logger registry to avoid creating duplicates
_loggers = {}


def get_logger(name: str) -> logging.Logger:
    """
    Get or create a named logger.

    Args:
        name: Logger name (e.g. "vector_db", "scheduler")

    Returns:
        Configured Logger instance
    """
    if name in _loggers:
        return _loggers[name]

    logger = logging.getLogger(f"openmem.{name}")
    logger.setLevel(logging.DEBUG)

    # Prevent duplicate handlers
    if not logger.handlers:
        # Console handler (WARNING+)
        console = logging.StreamHandler(sys.stdout)
        console.setLevel(logging.WARNING)
        console.setFormatter(logging.Formatter(
            "[%(name)s] %(levelname)s: %(message)s"
        ))
        logger.addHandler(console)

        # File handler (DEBUG+)
        try:
            file_handler = logging.FileHandler(str(LOG_FILE), encoding="utf-8")
            file_handler.setLevel(logging.DEBUG)
            file_handler.setFormatter(logging.Formatter(
                "%(asctime)s [%(name)s] %(levelname)s: %(message)s",
                datefmt="%Y-%m-%d %H:%M:%S",
            ))
            logger.addHandler(file_handler)
        except OSError:
            # Can't write to log file — console only
            pass

    _loggers[name] = logger
    return logger


def log_error(logger: logging.Logger, message: str, exc_info: bool = True):
    """
    Log an error with optional exception info.

    Replaces bare except:pass patterns.

    Usage:
        try:
            do_something()
        except Exception as e:
            log_error(log, "Failed to do something")
    """
    logger.error(message, exc_info=exc_info)


def log_warning(logger: logging.Logger, message: str):
    """Log a warning."""
    logger.warning(message)


def log_info(logger: logging.Logger, message: str):
    """Log an info message."""
    logger.info(message)


def log_debug(logger: logging.Logger, message: str):
    """Log a debug message."""
    logger.debug(message)


def safe_execute(func, fallback=None, logger=None, context="operation"):
    """
    Safely execute a function, logging any errors.

    Replaces try/except blocks that silently fail.

    Args:
        func: Callable to execute
        fallback: Value to return on failure
        logger: Logger instance (creates one if None)
        context: Description of what's being attempted

    Returns:
        Result of func() or fallback on error
    """
    if logger is None:
        logger = get_logger("safe_execute")

    try:
        return func()
    except Exception as e:
        log_error(logger, f"{context} failed: {e}")
        return fallback


# Module-level convenience functions
_default_log = get_logger("openmem")


def info(msg):
    _default_log.info(msg)


def warn(msg):
    _default_log.warning(msg)


def error(msg, exc_info=True):
    _default_log.error(msg, exc_info=exc_info)


def debug(msg):
    _default_log.debug(msg)
