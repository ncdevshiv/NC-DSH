from __future__ import annotations

import asyncio
import getpass
import inspect
import json
import re
import sys
import time
from argparse import Namespace
from typing import Any, Callable

from chatgpt_cdp_demo import (
    AnswerResult,
    CHATGPT_HELPER_JS,
    DemoError,
    MoliServe,
    read_json_url_no_proxy,
    redact_sensitive_text,
    redact_snapshot,
    start_moli,
    stop_moli,
)


Reporter = Callable[[str], None] | None
AnswerUpdate = Callable[[str], None] | None
WAITABLE_BLOCKING_REASONS = {"device-approval"}
CODE_BLOCKING_REASONS = {"email-verification", "verification-code"}

CHATGPT_LIVE_TRACE_JS = r"""
(() => {
  if (window.__lmChatGPTLiveTraceInstalled) return;
  window.__lmChatGPTLiveTraceInstalled = true;
  const events = [];
  const domMutationSamples = [];
  const maxEvents = 500;
  const maxConversationIdentityRecords = 48;
  const responseInfos = new WeakMap();
  const streamInfos = new WeakMap();
  const sourceStats = new Map();
  const idMapTraceSamples = [];
  const idMapTraceStats = {
    set: 0,
    setThreadKey: 0,
    get: 0,
    getHit: 0,
    getMiss: 0,
    getPresent: 0,
    getUndefined: 0,
    lastSetAt: 0,
    lastGetAt: 0,
  };
  const conversationIdentitySamples = [];
  const conversationMaterializationSamples = [];
  const conversationObjectIds = new WeakMap();
  const reactionObjectIds = new WeakMap();
  const conversationIdentityStats = {
    commitSamples: 0,
    changedSamples: 0,
    snapshotSamples: 0,
    lastSampleAt: 0,
    lastChangedAt: 0,
  };
  const conversationMaterializationStats = {
    commitSamples: 0,
    changedSamples: 0,
    streamSamples: 0,
    snapshotSamples: 0,
    lastSampleAt: 0,
    lastChangedAt: 0,
  };
  const navigationTraceSamples = [];
  const navigationTraceStats = {
    present: false,
    navigateCalls: 0,
    navigateErrors: 0,
    navigateCommitted: 0,
    navigateCommitRejected: 0,
    navigateFinished: 0,
    navigateFinishRejected: 0,
    navigateEvents: 0,
    currentEntryChanges: 0,
    navigateSuccess: 0,
    navigateError: 0,
    interceptCalls: 0,
    interceptHandlerStarted: 0,
    interceptHandlerSettled: 0,
    interceptHandlerRejected: 0,
    lastNavigateAt: 0,
    lastEventAt: 0,
    lastInterceptAt: 0,
    lastInterceptHandlerAt: 0,
  };
  const deepProbeCounts = new WeakMap();
  let seq = 0;
  let deepProbeCount = 0;
  let nextConversationObjectId = 0;
  let nextReactionObjectId = 0;
  let lastReactRoot = null;
  let lastDomSignature = '';
  let lastConversationIdentitySignature = '';
  let lastConversationMaterializationSignature = '';
  const eventLoopStats = {
    timeoutScheduled: 0,
    timeoutFired: 0,
    intervalScheduled: 0,
    intervalFired: 0,
    rafScheduled: 0,
    rafFired: 0,
    idleScheduled: 0,
    idleFired: 0,
    schedulerPostTaskScheduled: 0,
    schedulerPostTaskSettled: 0,
    microtaskScheduled: 0,
    microtaskFired: 0,
    heartbeat: 0,
    lastTimeoutFiredAt: 0,
    lastIntervalFiredAt: 0,
    lastRafFiredAt: 0,
    lastIdleFiredAt: 0,
    lastSchedulerPostTaskSettledAt: 0,
    lastMicrotaskFiredAt: 0,
    lastHeartbeatAt: 0,
  };
  const messageTaskStats = {
    messageChannelConstructed: 0,
    messagePortOnmessageSet: 0,
    messagePortOnmessageFired: 0,
    messagePortListenerAdded: 0,
    messagePortListenerFired: 0,
    messagePortPostMessage: 0,
    messagePortMessage: 0,
    messagePortStart: 0,
    windowPostMessage: 0,
    windowMessage: 0,
    lastMessageChannelConstructedAt: 0,
    lastMessagePortOnmessageSetAt: 0,
    lastMessagePortOnmessageFiredAt: 0,
    lastMessagePortListenerAddedAt: 0,
    lastMessagePortListenerFiredAt: 0,
    lastMessagePortPostMessageAt: 0,
    lastMessagePortMessageAt: 0,
    lastWindowPostMessageAt: 0,
    lastWindowMessageAt: 0,
  };
  const observerApiStats = {
    resizeConstructed: 0,
    resizeObserve: 0,
    resizeUnobserve: 0,
    resizeDisconnect: 0,
    resizeCallback: 0,
    resizeEntryCount: 0,
    intersectionConstructed: 0,
    intersectionObserve: 0,
    intersectionUnobserve: 0,
    intersectionDisconnect: 0,
    intersectionCallback: 0,
    intersectionEntryCount: 0,
    lastResizeCallbackAt: 0,
    lastIntersectionCallbackAt: 0,
  };
  const domMutationStats = {
    mutationObserverRecords: 0,
    mutationObserverInteresting: 0,
    appendChildCalls: 0,
    insertBeforeCalls: 0,
    replaceChildrenCalls: 0,
    interestingInserts: 0,
    interestingRemovals: 0,
    lastInterestingMutationAt: 0,
  };
  const reactFiberSamples = [];
  const reactCommitStats = {
    hookInstalled: false,
    hookPreexisting: false,
    rendererCount: 0,
    commitCount: 0,
    commitErrors: 0,
    lastCommitAt: 0,
    lastRendererId: 0,
    lastCommit: null,
  };

  function compactUrl(value) {
    try {
      const url = new URL(String(value), location.href);
      return `${url.origin}${url.pathname}`;
    } catch {
      return String(value || '').slice(0, 240);
    }
  }

  function isInterestingUrl(value) {
    const url = String(value || '');
    return url.includes('chatgpt.com/backend-api/f/conversation') ||
      url.includes('chatgpt.com/backend-api/conversation/') ||
      url.includes('chatgpt.com/backend-api/celsius/ws/user') ||
      url.includes('chatgpt.com/backend-api/sentinel/') ||
      url.includes('ws.chatgpt.com/');
  }

  function isTelemetryUrl(value) {
    const url = String(value || '');
    return url.includes('chatgpt.com/ces/') ||
      url.includes('chatgpt.com/backend-api/lat/');
  }

  function isConversationPostUrl(value) {
    try {
      const url = new URL(String(value || ''), window.location.href);
      return url.hostname === 'chatgpt.com' &&
        url.pathname === '/backend-api/f/conversation';
    } catch {
      return false;
    }
  }

  function nowMs() {
    try {
      return Math.round(performance.now());
    } catch {
      return Date.now();
    }
  }

  function record(type, data = {}) {
    events.push({seq: ++seq, t: nowMs(), type, ...data});
    if (events.length > maxEvents) events.splice(0, events.length - maxEvents);
  }

  function pushDomMutationSample(sample) {
    domMutationSamples.push(sample);
    if (domMutationSamples.length > 80) {
      domMutationSamples.splice(0, domMutationSamples.length - 80);
    }
  }

  function pushReactFiberSample(sample) {
    reactFiberSamples.push(sample);
    if (reactFiberSamples.length > 80) {
      reactFiberSamples.splice(0, reactFiberSamples.length - 80);
    }
  }

  function pushIdMapTraceSample(sample) {
    idMapTraceSamples.push({t: nowMs(), ...sample});
    if (idMapTraceSamples.length > 80) {
      idMapTraceSamples.splice(0, idMapTraceSamples.length - 80);
    }
  }

  function pushNavigationTraceSample(sample) {
    navigationTraceSamples.push({t: nowMs(), ...sample});
    if (navigationTraceSamples.length > 80) {
      navigationTraceSamples.splice(0, navigationTraceSamples.length - 80);
    }
  }

  function pushConversationIdentitySample(sample, options = {}) {
    const changed = !!options.changed;
    conversationIdentitySamples.push({t: nowMs(), changed, ...sample});
    if (conversationIdentitySamples.length > 80) {
      conversationIdentitySamples.splice(0, conversationIdentitySamples.length - 80);
    }
    conversationIdentityStats.lastSampleAt = nowMs();
    if (sample.reason === 'commit') conversationIdentityStats.commitSamples += 1;
    if (sample.reason === 'snapshot') conversationIdentityStats.snapshotSamples += 1;
    if (changed) {
      conversationIdentityStats.changedSamples += 1;
      conversationIdentityStats.lastChangedAt = nowMs();
    }
  }

  function pushConversationMaterializationSample(sample, options = {}) {
    const changed = !!options.changed;
    conversationMaterializationSamples.push({t: nowMs(), changed, ...sample});
    if (conversationMaterializationSamples.length > 80) {
      conversationMaterializationSamples.splice(0, conversationMaterializationSamples.length - 80);
    }
    conversationMaterializationStats.lastSampleAt = nowMs();
    if (sample.reason === 'commit') conversationMaterializationStats.commitSamples += 1;
    if (sample.reason === 'snapshot') conversationMaterializationStats.snapshotSamples += 1;
    if (String(sample.reason || '').startsWith('stream-read')) {
      conversationMaterializationStats.streamSamples += 1;
    }
    if (changed) {
      conversationMaterializationStats.changedSamples += 1;
      conversationMaterializationStats.lastChangedAt = nowMs();
    }
  }

  function textOf(el) {
    if (!el) return '';
    return String(el.value || el.textContent || el.getAttribute?.('aria-label') || '').trim();
  }

  function selectorSummary(selector) {
    try {
      const elements = [...document.querySelectorAll(selector)];
      const textLengths = elements.map((el) => textOf(el).length).filter((length) => length > 0);
      return {
        count: elements.length,
        textCount: textLengths.length,
        latestTextLen: textLengths.length ? textLengths[textLengths.length - 1] : 0,
        maxTextLen: textLengths.length ? Math.max(...textLengths) : 0,
      };
    } catch {
      return {count: -1, textCount: 0, latestTextLen: 0, maxTextLen: 0};
    }
  }

  function selectorCensus() {
    return {
      assistantRole: selectorSummary('[data-message-author-role="assistant"]'),
      userRole: selectorSummary('[data-message-author-role="user"]'),
      messageRole: selectorSummary('[data-message-author-role]'),
      messageId: selectorSummary('[data-message-id]'),
      conversationTurn: selectorSummary('[data-testid*="conversation-turn" i]'),
      article: selectorSummary('article, [role="article"]'),
      markdown: selectorSummary('.markdown, [class*="markdown" i]'),
      main: selectorSummary('main'),
      composer: selectorSummary('#prompt-textarea, textarea, [contenteditable="true"]'),
      stopButton: selectorSummary('button[data-testid*="stop" i], button[aria-label*="Stop" i], button[aria-label*="Cancel" i]'),
      appRoot: selectorSummary('#__next, #root, [data-reactroot]'),
    };
  }

  function elementBrief(el) {
    if (!el) return null;
    const className = typeof el.className === 'string' ? el.className : '';
    return {
      tag: el.tagName || '',
      id: el.id || '',
      role: el.getAttribute?.('role') || '',
      testid: el.getAttribute?.('data-testid') || '',
      dataTurn: el.getAttribute?.('data-turn') || '',
      ariaHidden: el.getAttribute?.('aria-hidden') || '',
      inert: el.hasAttribute?.('inert') || false,
      classHints: className
        .split(/\s+/)
        .filter((name) => /thread|conversation|message|turn|composer|markdown|viewport|virtual|list/i.test(name))
        .slice(0, 8),
      textLen: textOf(el).length,
      childCount: el.children?.length || 0,
    };
  }

  function nodeBrief(node) {
    if (!node) return null;
    if (node.nodeType === Node.ELEMENT_NODE) return elementBrief(node);
    return {
      nodeType: node.nodeType,
      nodeName: node.nodeName || '',
      textLen: textOf(node).length,
    };
  }

  function isInterestingDomNode(node) {
    if (!node) return false;
    if (node.nodeType === Node.TEXT_NODE) {
      return /OK|assistant|message|conversation|thread/i.test(String(node.nodeValue || ''));
    }
    if (node.nodeType !== Node.ELEMENT_NODE) return false;
    const el = node;
    const className = typeof el.className === 'string' ? el.className : '';
    if (
      el.id === 'thread' ||
      el.hasAttribute?.('data-turn') ||
      el.hasAttribute?.('data-message-id') ||
      el.hasAttribute?.('data-message-author-role') ||
      el.getAttribute?.('role') === 'article' ||
      el.getAttribute?.('data-testid')?.includes('conversation') ||
      /thread|conversation|message|turn|markdown|composer/i.test(className)
    ) {
      return true;
    }
    try {
      return !!el.querySelector?.(
        '#thread, [data-turn], [data-message-id], [data-message-author-role], [role="article"], .markdown'
      );
    } catch {
      return false;
    }
  }

  function mutationRecordBrief(record) {
    const added = [...record.addedNodes || []].filter(isInterestingDomNode).slice(0, 8);
    const removed = [...record.removedNodes || []].filter(isInterestingDomNode).slice(0, 8);
    return {
      type: record.type,
      target: nodeBrief(record.target),
      attributeName: record.attributeName || '',
      added: added.map(nodeBrief),
      removed: removed.map(nodeBrief),
    };
  }

  function domTreeProbe() {
    const thread = document.getElementById('thread');
    const main = document.querySelector('main');
    return {
      thread: elementBrief(thread),
      threadDescendantCount: thread ? thread.querySelectorAll('*').length : 0,
      dataTurnCount: document.querySelectorAll('[data-turn]').length,
      articleCount: document.querySelectorAll('article, [role="article"]').length,
      mainChildren: main ? [...main.children].slice(0, 12).map(elementBrief) : [],
      bodyChildren: [...document.body?.children || []].slice(0, 12).map(elementBrief),
    };
  }

  function objectKeys(value, limit = 12) {
    try {
      return value && typeof value === 'object' ? Object.keys(value).slice(0, limit) : [];
    } catch {
      return [];
    }
  }

  function ownKeySummary(value, limit = 12) {
    try {
      if (!value || typeof value !== 'object') return [];
      return Reflect.ownKeys(value)
        .slice(0, limit)
        .map((key) => typeof key === 'symbol' ? `symbol:${String(key.description || '')}` : String(key));
    } catch {
      return [];
    }
  }

  function mapLikeSize(value) {
    if (!value) return 0;
    if (typeof value.size === 'number') return value.size;
    if (Array.isArray(value)) return value.length;
    if (typeof value === 'object') return objectKeys(value, 500).length;
    return 0;
  }

  function countBy(values) {
    const counts = {};
    for (const value of values) {
      const key = String(value || '');
      counts[key] = (counts[key] || 0) + 1;
    }
    return counts;
  }

  function valueShape(value, depth = 0) {
    if (value === null) return {kind: 'null'};
    if (value === undefined) return {kind: 'undefined'};
    if (depth > 2) return {kind: 'nested'};
    if (typeof value === 'function') {
      return {
        kind: 'function',
        name: String(value.name || '').slice(0, 80),
        length: Number(value.length) || 0,
      };
    }
    if (Array.isArray(value)) {
      return {
        kind: 'array',
        length: value.length,
        items: value.slice(0, 4).map((item) => valueShape(item, depth + 1)),
      };
    }
    if (value && typeof value === 'object') {
      const keys = objectKeys(value, 16);
      const ownKeys = ownKeySummary(value, 16);
      const fields = {};
      for (const key of [
        'type',
        'kind',
        'status',
        'state',
        'role',
        'author',
        'message_type',
        'fetchStatus',
        'isPending',
        'isSuccess',
        'isError',
        'isLoading',
        'version',
        '_treeVersion',
      ]) {
        const field = value[key];
        if (typeof field === 'string' || typeof field === 'number' || typeof field === 'boolean') {
          fields[key] = field;
        }
      }
      return {
        kind: 'object',
        tag: Object.prototype.toString.call(value),
        constructorName: value.constructor?.name || '',
        keys,
        ownKeys,
        fields,
      };
    }
    if (typeof value === 'string') {
      const shape = {kind: 'string', length: value.length};
      const id = idShape(value);
      if (id !== `string:${value.length}`) shape.id = id;
      return shape;
    }
    if (typeof value === 'number' || typeof value === 'boolean') {
      return {kind: typeof value, scalar: value};
    }
    return {kind: typeof value};
  }

  function propKeyLabel(key) {
    return typeof key === 'symbol' ? `symbol:${String(key.description || '')}` : String(key);
  }

  function ownValueShapes(value, limit = 16) {
    const shapes = [];
    if (!value || typeof value !== 'object') return shapes;
    try {
      const keys = Reflect.ownKeys(value);
      for (let index = 0; index < keys.length && shapes.length < limit; index += 1) {
        const key = keys[index];
        const label = propKeyLabel(key);
        const entry = {key: label, index};
        try {
          const descriptor = Object.getOwnPropertyDescriptor(value, key);
          if (!descriptor) {
            entry.missingDescriptor = true;
          } else if ('value' in descriptor) {
            entry.shape = valueShape(descriptor.value, 1);
          } else {
            entry.accessor = {
              get: typeof descriptor.get,
              set: typeof descriptor.set,
            };
          }
        } catch (error) {
          entry.error = String(error?.name || error);
        }
        shapes.push(entry);
      }
    } catch (error) {
      shapes.push({error: String(error?.name || error)});
    }
    return shapes;
  }

  function collectionSummary(value, limit = 6) {
    const summary = valueShape(value, 1);
    try {
      if (value instanceof Map) {
        summary.collectionKind = 'Map';
        summary.size = value.size;
        summary.entries = [];
        let index = 0;
        for (const [key, item] of value.entries()) {
          if (index >= limit) break;
          summary.entries.push({
            key: valueShape(key, 1),
            value: valueShape(item, 1),
            valueOwnValueShapes: ownValueShapes(item, 8),
          });
          index += 1;
        }
      } else if (value instanceof Set) {
        summary.collectionKind = 'Set';
        summary.size = value.size;
        summary.entries = [];
        let index = 0;
        for (const item of value.values()) {
          if (index >= limit) break;
          summary.entries.push({
            value: valueShape(item, 1),
            valueOwnValueShapes: ownValueShapes(item, 8),
          });
          index += 1;
        }
      } else if (Array.isArray(value)) {
        summary.collectionKind = 'Array';
        summary.entries = value.slice(0, limit).map((item, index) => ({
          index,
          value: valueShape(item, 1),
          valueOwnValueShapes: ownValueShapes(item, 8),
        }));
      } else if (value && typeof value === 'object') {
        summary.collectionKind = 'Object';
        summary.entries = objectKeys(value, limit).map((key) => ({
          key,
          value: valueShape(value[key], 1),
          valueOwnValueShapes: ownValueShapes(value[key], 8),
        }));
      }
    } catch (error) {
      summary.collectionError = String(error?.name || error);
    }
    return summary;
  }

  function semanticScalarFields(value) {
    const fields = {};
    if (!value || typeof value !== 'object') return fields;
    for (const key of [
      'type',
      'kind',
      'status',
      'state',
      'role',
      'message_type',
      'fetchStatus',
      'isPending',
      'isSuccess',
      'isError',
      'isLoading',
      'isCompletionInProgress',
      'isNewThread',
      'hasUserMessage',
      'hasAssistantMessage',
      'conversation_id',
      'conversationId',
      'thread_id',
      'threadId',
      'clientThreadId',
      'serverThreadId',
      'message_id',
      'messageId',
      'parent_id',
      'parentMessageId',
      'id',
    ]) {
      try {
        const item = value[key];
        if (typeof item === 'string') {
          fields[key] = /(^id$|id$|Id$|_id$)/.test(key) ? idShape(item) : `string:${item.length}`;
        } else if (typeof item === 'number' || typeof item === 'boolean') {
          fields[key] = item;
        }
      } catch {}
    }
    try {
      const author = value.author;
      if (author && typeof author === 'object') {
        const role = author.role || author.type;
        if (typeof role === 'string') fields.authorRole = role;
      }
    } catch {}
    try {
      const content = value.content;
      if (content && typeof content === 'object') {
        fields.contentKeys = objectKeys(content, 8);
        if (typeof content.content_type === 'string') fields.contentType = content.content_type;
        if (Array.isArray(content.parts)) {
          fields.contentPartCount = content.parts.length;
          fields.contentPartLengths = content.parts.slice(0, 4).map((part) => {
            if (typeof part === 'string') return part.length;
            if (part && typeof part === 'object') return objectKeys(part, 6).join(',');
            return typeof part;
          });
        }
      }
    } catch {}
    try {
      if (Object.prototype.hasOwnProperty.call(value, 'hasValue')) {
        fields.hasValue = !!value.hasValue;
      }
      if (Object.prototype.hasOwnProperty.call(value, 'value')) {
        const boxed = value.value;
        fields.valueShape = valueShape(boxed, 1);
        if (typeof boxed === 'number' || typeof boxed === 'boolean') {
          fields.valueScalar = boxed;
        } else if (typeof boxed === 'string') {
          fields.valueId = idShape(boxed);
        }
      }
    } catch {}
    return fields;
  }

  function isReactionStoreLike(value) {
    if (!value || typeof value !== 'object') return false;
    let matchingKeys = 0;
    for (const key of [
      'reaction',
      'onStoreChange',
      'stateVersion',
      'name',
      'lastValue',
      'evaluate',
      'subscribe',
      'getSnapshot',
    ]) {
      try {
        if (Object.prototype.hasOwnProperty.call(value, key)) matchingKeys += 1;
      } catch {}
    }
    try {
      return matchingKeys >= 4 &&
        (typeof value.subscribe === 'function' || typeof value.getSnapshot === 'function');
    } catch {
      return matchingKeys >= 5;
    }
  }

  function reactionObjectLabel(value) {
    if (!value || typeof value !== 'object') return '';
    try {
      let label = reactionObjectIds.get(value);
      if (!label) {
        label = `reaction:${++nextReactionObjectId}`;
        reactionObjectIds.set(value, label);
      }
      return label;
    } catch {
      return '';
    }
  }

  function safeScalarShape(value, key = '') {
    if (value === null) return {kind: 'null'};
    if (value === undefined) return {kind: 'undefined'};
    if (typeof value === 'string') {
      const shape = {kind: 'string', length: value.length};
      if (/id|thread|conversation|message/i.test(key)) shape.id = idShape(value);
      if (
        /status|state|role|kind|type/i.test(key) &&
        /^[A-Za-z_-]{1,32}$/.test(value)
      ) {
        shape.scalar = value;
      }
      return shape;
    }
    if (typeof value === 'number' || typeof value === 'boolean') {
      return {kind: typeof value, scalar: value};
    }
    return valueShape(value, 1);
  }

  function queryLikeValueSummary(value) {
    const summary = valueShape(value, 1);
    if (!value || typeof value !== 'object') return summary;
    const fields = {};
    for (const key of [
      'status',
      'fetchStatus',
      'isPending',
      'isSuccess',
      'isError',
      'isInitialLoading',
      'isLoading',
      'isFetched',
      'isFetching',
      'isStale',
      'isPlaceholderData',
      'dataUpdatedAt',
      'errorUpdatedAt',
      'failureCount',
      'failureReason',
      'conversationId',
      'threadId',
      'clientThreadId',
      'serverThreadId',
      'messageId',
      'id',
    ]) {
      try {
        if (Object.prototype.hasOwnProperty.call(value, key)) {
          fields[key] = safeScalarShape(value[key], key);
        }
      } catch (error) {
        fields[key] = {error: String(error?.name || error)};
      }
    }
    if (Object.keys(fields).length) summary.queryFields = fields;
    for (const key of ['data', 'current', 'value', 'error', 'promise']) {
      try {
        if (Object.prototype.hasOwnProperty.call(value, key)) {
          summary[`${key}Shape`] = valueShape(value[key], 1);
          const nestedFields = semanticScalarFields(value[key]);
          if (Object.keys(nestedFields).length) {
            summary[`${key}Fields`] = nestedFields;
          }
        }
      } catch (error) {
        summary[`${key}Error`] = String(error?.name || error);
      }
    }
    return summary;
  }

  function reactionStoreDetail(value) {
    if (!isReactionStoreLike(value)) return null;
    const detail = {
      kind: 'reaction-store',
      object: reactionObjectLabel(value),
      ownKeys: ownKeySummary(value, 16),
      hasSubscribe: false,
      hasGetSnapshot: false,
      hasEvaluate: false,
      hasOnStoreChange: false,
    };
    try {
      detail.hasSubscribe = typeof value.subscribe === 'function';
      detail.hasGetSnapshot = typeof value.getSnapshot === 'function';
      detail.hasEvaluate = typeof value.evaluate === 'function';
      detail.hasOnStoreChange = typeof value.onStoreChange === 'function';
    } catch {}
    for (const key of ['stateVersion', 'name']) {
      try {
        if (Object.prototype.hasOwnProperty.call(value, key)) {
          detail[key] = safeScalarShape(value[key], key);
        }
      } catch (error) {
        detail[`${key}Error`] = String(error?.name || error);
      }
    }
    try {
      if (Object.prototype.hasOwnProperty.call(value, 'lastValue')) {
        detail.lastValue = queryLikeValueSummary(value.lastValue);
      }
    } catch (error) {
      detail.lastValueError = String(error?.name || error);
    }
    try {
      if (typeof value.getSnapshot === 'function') {
        detail.snapshot = queryLikeValueSummary(value.getSnapshot());
      }
    } catch (error) {
      detail.snapshotError = String(error?.name || error);
    }
    try {
      if (Object.prototype.hasOwnProperty.call(value, 'reaction')) {
        detail.reactionShape = valueShape(value.reaction, 1);
      }
    } catch (error) {
      detail.reactionError = String(error?.name || error);
    }
    return detail;
  }

  function semanticValueDetail(value, depth = 0) {
    const detail = valueShape(value, 1);
    if (depth >= 2 || !value || (typeof value !== 'object' && typeof value !== 'function')) {
      return detail;
    }
    if (Array.isArray(value)) {
      detail.collectionKind = 'Array';
      detail.entries = value.slice(0, 6).map((item, index) => ({
        index,
        value: semanticValueDetail(item, depth + 1),
      }));
      return detail;
    }
    if (value instanceof Map) {
      detail.collectionKind = 'Map';
      detail.size = value.size;
      detail.entries = [];
      let index = 0;
      for (const [key, item] of value.entries()) {
        if (index >= 6) break;
        detail.entries.push({
          key: semanticValueDetail(key, depth + 1),
          value: semanticValueDetail(item, depth + 1),
        });
        index += 1;
      }
      return detail;
    }
    if (value instanceof Set) {
      detail.collectionKind = 'Set';
      detail.size = value.size;
      detail.entries = [];
      let index = 0;
      for (const item of value.values()) {
        if (index >= 6) break;
        detail.entries.push({value: semanticValueDetail(item, depth + 1)});
        index += 1;
      }
      return detail;
    }
    if (typeof value === 'object') {
      detail.fields = {...(detail.fields || {}), ...semanticScalarFields(value)};
      const reactionDetail = reactionStoreDetail(value);
      if (reactionDetail) detail.reactionStore = reactionDetail;
      const conversationDetail = conversationResultDetail(value, depth + 1);
      if (conversationDetail) detail.conversationDetail = conversationDetail;
      const selectedValues = {};
      const startedAt = nowMs();
      try {
        for (const key of Reflect.ownKeys(value)) {
          if (Object.keys(selectedValues).length >= 12 || nowMs() - startedAt > 80) break;
          const label = propKeyLabel(key);
          if (
            !/conversation|thread|tree|turn|message|display|item|current|value|status|loading|content|author|role|server|client/i.test(label) ||
            label === 'children'
          ) {
            continue;
          }
          try {
            selectedValues[label] = semanticValueDetail(value[key], depth + 1);
          } catch (error) {
            selectedValues[label] = {error: String(error?.name || error)};
          }
        }
      } catch (error) {
        selectedValues.error = String(error?.name || error);
      }
      if (Object.keys(selectedValues).length) detail.selectedValues = selectedValues;
    }
    return detail;
  }

  function selectedNamedValueShapes(value, patterns, limit = 12) {
    const selected = {};
    if (!value || typeof value !== 'object') return selected;
    const startedAt = nowMs();
    try {
      const keys = Reflect.ownKeys(value);
      for (const key of keys) {
        if (Object.keys(selected).length >= limit || nowMs() - startedAt > 100) break;
        const label = propKeyLabel(key);
        if (!patterns.some((pattern) => pattern.test(label))) continue;
        let propValue;
        try {
          propValue = value[key];
        } catch (error) {
          selected[label] = {error: String(error?.name || error)};
          continue;
        }
        const shape = valueShape(propValue, 1);
        const detail = conversationResultDetail(propValue, 1);
        selected[label] = detail ? {...shape, detail} : shape;
      }
    } catch (error) {
      selected.error = String(error?.name || error);
    }
    return selected;
  }

  function storeLikeSummaries(value, limit = 8) {
    const stores = [];
    if (!value || typeof value !== 'object') return stores;
    const startedAt = nowMs();
    try {
      const keys = Reflect.ownKeys(value);
      for (const key of keys) {
        if (stores.length >= limit || nowMs() - startedAt > 150) break;
        let candidate;
        try {
          candidate = value[key];
        } catch (error) {
          stores.push({key: propKeyLabel(key), error: String(error?.name || error)});
          continue;
        }
        if (
          !candidate ||
          typeof candidate !== 'object' ||
          typeof candidate.getState !== 'function' ||
          typeof candidate.subscribe !== 'function'
        ) {
          continue;
        }
        const summary = {
          key: propKeyLabel(key),
          shape: valueShape(candidate, 1),
        };
        try {
          if (candidate._listeners instanceof Set || candidate._listeners instanceof Map) {
            summary.listenerCount = candidate._listeners.size;
          }
        } catch {}
        try {
          const state = candidate.getState();
          summary.stateShape = valueShape(state, 1);
          summary.stateOwnValueShapes = ownValueShapes(state, 16);
          summary.selectedStateValues = selectedNamedValueShapes(
            state,
            [/conversation/i, /thread/i, /tree/i, /turn/i, /display/i, /message/i],
            12,
          );
        } catch (error) {
          summary.stateError = String(error?.name || error);
        }
        stores.push(summary);
      }
    } catch (error) {
      stores.push({error: String(error?.name || error)});
    }
    return stores;
  }

  function idMapObjectSummary(value, limit = 12) {
    if (!value || typeof value !== 'object') return null;
    const keys = objectKeys(value, limit);
    return {
      count: objectKeys(value, 500).length,
      entries: keys.map((key) => {
        let mapped = '';
        try {
          mapped = typeof value[key] === 'string' ? value[key] : '';
        } catch {}
        return {key: idShape(key), value: mapped ? idShape(mapped) : ''};
      }),
    };
  }

  function threadStoreStateDetail(value) {
    if (!value || typeof value !== 'object') return null;
    if (
      !Object.prototype.hasOwnProperty.call(value, 'clientNewThreadIdToServerIdMapping') &&
      !Object.prototype.hasOwnProperty.call(value, 'threads')
    ) {
      return null;
    }
    const detail = {};
    try {
      detail.mapping = idMapObjectSummary(value.clientNewThreadIdToServerIdMapping, 16);
    } catch (error) {
      detail.mappingError = String(error?.name || error);
    }
    try {
      const threads = value.threads;
      detail.threadCount = threads && typeof threads === 'object' ? objectKeys(threads, 1000).length : 0;
      detail.threadKeys = threads && typeof threads === 'object'
        ? objectKeys(threads, 16).map((key) => idShape(key))
        : [];
    } catch (error) {
      detail.threadsError = String(error?.name || error);
    }
    return detail;
  }

  function hookValueDetail(value) {
    const detail = valueShape(value, 1);
    if (!value || typeof value !== 'object') return detail;
    try {
      const reactionDetail = reactionStoreDetail(value);
      if (reactionDetail) detail.reactionStore = reactionDetail;
    } catch {}
    try {
      const threadStore = threadStoreStateDetail(value);
      if (threadStore) detail.threadStore = threadStore;
    } catch {}
    try {
      const conversationDetail = conversationResultDetail(value, 1);
      if (conversationDetail) detail.conversationDetail = conversationDetail;
    } catch {}
    try {
      detail.selectedValues = selectedNamedValueShapes(
        value,
        [/conversation/i, /thread/i, /tree/i, /turn/i, /display/i, /message/i],
        10,
      );
    } catch {}
    return detail;
  }

  function hookStateSummary(fiber, limit = 8) {
    const hooks = [];
    const seen = new Set();
    let current = null;
    try {
      current = fiber?.memoizedState || null;
    } catch {
      return hooks;
    }
    while (current && hooks.length < limit && !seen.has(current)) {
      seen.add(current);
      const entry = {
        index: hooks.length,
        hookKeys: ownKeySummary(current, 12),
      };
      try {
        entry.memoizedState = hookValueDetail(current.memoizedState);
      } catch (error) {
        entry.memoizedStateError = String(error?.name || error);
      }
      try {
        if (current.baseState !== undefined) {
          entry.baseState = hookValueDetail(current.baseState);
        }
      } catch (error) {
        entry.baseStateError = String(error?.name || error);
      }
      try {
        if (current.queue) {
          entry.queueShape = valueShape(current.queue, 1);
          if (current.queue.value !== undefined) {
            entry.queueValue = hookValueDetail(current.queue.value);
          }
          if (current.queue.lastRenderedState !== undefined) {
            entry.lastRenderedState = hookValueDetail(current.queue.lastRenderedState);
          }
          if (typeof current.queue.getSnapshot === 'function') {
            try {
              const snapshot = current.queue.getSnapshot();
              if (snapshot !== undefined) entry.queueSnapshot = hookValueDetail(snapshot);
            } catch (error) {
              entry.queueSnapshotError = String(error?.name || error);
            }
          }
        }
      } catch (error) {
        entry.queueError = String(error?.name || error);
      }
      hooks.push(entry);
      try {
        current = current.next || null;
      } catch {
        break;
      }
    }
    return hooks;
  }

  function hookSemanticSummary(fiber, limit = 24) {
    const hooks = [];
    const seen = new Set();
    let current = null;
    try {
      current = fiber?.memoizedState || null;
    } catch {
      return hooks;
    }
    while (current && hooks.length < limit && !seen.has(current)) {
      seen.add(current);
      const entry = {
        index: hooks.length,
        hookKeys: ownKeySummary(current, 12),
      };
      try {
        entry.memoizedState = semanticValueDetail(current.memoizedState);
      } catch (error) {
        entry.memoizedStateError = String(error?.name || error);
      }
      try {
        if (current.baseState !== undefined) {
          entry.baseState = semanticValueDetail(current.baseState);
        }
      } catch (error) {
        entry.baseStateError = String(error?.name || error);
      }
      try {
        if (current.queue) {
          entry.queueShape = valueShape(current.queue, 1);
          if (current.queue.value !== undefined) {
            entry.queueValue = semanticValueDetail(current.queue.value);
          }
          if (current.queue.lastRenderedState !== undefined) {
            entry.lastRenderedState = semanticValueDetail(current.queue.lastRenderedState);
          }
          if (typeof current.queue.getSnapshot === 'function') {
            try {
              const snapshot = current.queue.getSnapshot();
              if (snapshot !== undefined) entry.queueSnapshot = semanticValueDetail(snapshot);
            } catch (error) {
              entry.queueSnapshotError = String(error?.name || error);
            }
          }
        }
      } catch (error) {
        entry.queueError = String(error?.name || error);
      }
      hooks.push(entry);
      try {
        current = current.next || null;
      } catch {
        break;
      }
    }
    return hooks;
  }

  function threadRendererProbes(limit = 16) {
    const probes = [];
    const stack = [];
    const seen = new Set();
    try {
      if (lastReactRoot?.current) stack.push({fiber: lastReactRoot.current, depth: 0});
      else if (lastReactRoot) stack.push({fiber: lastReactRoot, depth: 0});
    } catch {}
    while (stack.length && probes.length < limit && seen.size < 6000) {
      const {fiber, depth} = stack.pop();
      if (!fiber || seen.has(fiber)) continue;
      seen.add(fiber);
      let name = '';
      let props = null;
      let hints = [];
      try {
        name = reactFiberName(fiber);
        props = fiber.memoizedProps;
        hints = reactFiberHints(fiber, name, props);
      } catch {}
      if (/^(d8|T2|Zqn|qJn|pY|iyr|u4n|qDr|\$Ar|NFe|BFe|e8)$/.test(name) ||
          isConversationThreadListFiber(name, props)) {
        probes.push({
          depth,
          name,
          tag: Number(fiber.tag),
          hints,
          props: compactFiberPropsForTrace(props),
          sourceHint: shouldRecordFiberSourceHint(name, props) ? reactFiberSourceHint(fiber) : undefined,
          hooks: hookSemanticSummary(fiber, 24),
        });
      }
      try {
        if (fiber.sibling) stack.push({fiber: fiber.sibling, depth});
        if (fiber.child) stack.push({fiber: fiber.child, depth: depth + 1});
      } catch {}
    }
    return {
      rootPresent: !!lastReactRoot,
      visited: seen.size,
      count: probes.length,
      probes,
    };
  }

  function threadStoreHookSnapshots(limit = 12) {
    const snapshots = [];
    const stack = [];
    const seenFibers = new Set();
    try {
      if (lastReactRoot?.current) stack.push({fiber: lastReactRoot.current, depth: 0});
      else if (lastReactRoot) stack.push({fiber: lastReactRoot, depth: 0});
    } catch {}

    function pushIfThreadStore(fiber, depth, hookIndex, source, candidate) {
      if (snapshots.length >= limit || !candidate || typeof candidate !== 'object') return;
      const detail = threadStoreStateDetail(candidate);
      if (!detail) return;
      snapshots.push({
        depth,
        name: reactFiberName(fiber),
        tag: Number(fiber.tag),
        hookIndex,
        source,
        detail: hookValueDetail(candidate),
      });
    }

    while (stack.length && snapshots.length < limit && seenFibers.size < 5000) {
      const {fiber, depth} = stack.pop();
      if (!fiber || seenFibers.has(fiber)) continue;
      seenFibers.add(fiber);
      try {
        let hook = fiber.memoizedState || null;
        const seenHooks = new Set();
        for (let hookIndex = 0; hook && hookIndex < 16 && !seenHooks.has(hook); hookIndex += 1) {
          seenHooks.add(hook);
          try {
            pushIfThreadStore(fiber, depth, hookIndex, 'memoizedState', hook.memoizedState);
            pushIfThreadStore(fiber, depth, hookIndex, 'baseState', hook.baseState);
            if (hook.queue) {
              pushIfThreadStore(fiber, depth, hookIndex, 'queue.value', hook.queue.value);
              pushIfThreadStore(
                fiber,
                depth,
                hookIndex,
                'queue.lastRenderedState',
                hook.queue.lastRenderedState,
              );
              if (typeof hook.queue.getSnapshot === 'function') {
                try {
                  pushIfThreadStore(
                    fiber,
                    depth,
                    hookIndex,
                    'queue.getSnapshot',
                    hook.queue.getSnapshot(),
                  );
                } catch {}
              }
            }
          } catch {}
          hook = hook.next || null;
        }
      } catch {}
      try {
        if (fiber.sibling) stack.push({fiber: fiber.sibling, depth});
        if (fiber.child) stack.push({fiber: fiber.child, depth: depth + 1});
      } catch {}
    }
    return {
      rootPresent: !!lastReactRoot,
      visited: seenFibers.size,
      count: snapshots.length,
      snapshots,
    };
  }

  function reactionStoreProbes(limit = 96) {
    const probes = [];
    const stack = [];
    const seenFibers = new Set();
    const seenCandidates = new WeakSet();
    try {
      const rootFiber = lastReactRoot?.current || lastReactRoot || null;
      if (rootFiber) stack.push({fiber: rootFiber, depth: 0, source: 'root'});
    } catch {}
    try {
      const threadFiber = fiberForDomNode(document.getElementById('thread'));
      if (threadFiber) stack.push({fiber: threadFiber, depth: 0, source: 'thread'});
    } catch {}

    function pushCandidate(fiber, depth, hookIndex, source, candidate) {
      if (probes.length >= limit || !isReactionStoreLike(candidate)) return;
      try {
        if (seenCandidates.has(candidate)) return;
        seenCandidates.add(candidate);
      } catch {}
      let name = '';
      let props = null;
      let hints = [];
      try {
        name = reactFiberName(fiber);
        props = fiber.memoizedProps;
        hints = reactFiberHints(fiber, name, props);
      } catch {}
      probes.push({
        source,
        depth,
        name,
        tag: Number(fiber.tag),
        tagLabel: reactFiberTagLabel(Number(fiber.tag)),
        hints,
        path: fiberPathSummary(fiber, 10),
        hookIndex,
        hookSource: source,
        selectedThreadProps: identityThreadPropShapes(props),
        detail: reactionStoreDetail(candidate),
      });
    }

    while (stack.length && probes.length < limit && seenFibers.size < 9000) {
      const {fiber, depth, source} = stack.pop();
      if (!fiber || seenFibers.has(fiber)) continue;
      seenFibers.add(fiber);
      try {
        let hook = fiber.memoizedState || null;
        const seenHooks = new Set();
        for (let hookIndex = 0; hook && hookIndex < 32 && !seenHooks.has(hook); hookIndex += 1) {
          seenHooks.add(hook);
          try {
            pushCandidate(fiber, depth, hookIndex, `${source}:memoizedState`, hook.memoizedState);
            if (hook.memoizedState && typeof hook.memoizedState === 'object') {
              pushCandidate(
                fiber,
                depth,
                hookIndex,
                `${source}:memoizedState.current`,
                hook.memoizedState.current,
              );
            }
            pushCandidate(fiber, depth, hookIndex, `${source}:baseState`, hook.baseState);
            if (hook.queue) {
              pushCandidate(fiber, depth, hookIndex, `${source}:queue.value`, hook.queue.value);
              pushCandidate(
                fiber,
                depth,
                hookIndex,
                `${source}:queue.lastRenderedState`,
                hook.queue.lastRenderedState,
              );
              if (typeof hook.queue.getSnapshot === 'function') {
                try {
                  pushCandidate(
                    fiber,
                    depth,
                    hookIndex,
                    `${source}:queue.getSnapshot`,
                    hook.queue.getSnapshot(),
                  );
                } catch {}
              }
            }
          } catch {}
          hook = hook.next || null;
        }
      } catch {}
      try {
        if (fiber.sibling) stack.push({fiber: fiber.sibling, depth, source});
        if (fiber.child) stack.push({fiber: fiber.child, depth: depth + 1, source});
      } catch {}
    }
    return {
      rootPresent: !!lastReactRoot,
      visited: seenFibers.size,
      count: probes.length,
      probes,
    };
  }

  function conversationResultDetail(value, depth = 0) {
    if (!value || typeof value !== 'object') return null;
    const detail = {keys: objectKeys(value, 20), ownKeys: ownKeySummary(value, 20)};
    try {
      if (value.tree && typeof value.tree === 'object') {
        detail.treeShape = valueShape(value.tree, 1);
        detail.treeOwnValueShapes = ownValueShapes(value.tree, 16);
        detail.treePrototypeMethods = prototypeMethodSummary(value.tree, 24);
        try {
          detail.treeCurrentLeafId = idShape(String(value.tree.currentLeafId || ''));
        } catch (error) {
          detail.treeCurrentLeafIdError = String(error?.name || error);
        }
        try {
          detail.treeNodes = collectionSummary(value.tree.nodes, 6);
        } catch (error) {
          detail.treeNodesError = String(error?.name || error);
        }
        try {
          if (typeof value.tree.getDisplayItems === 'function') {
            detail.treeDisplayItems = collectionSummary(
              value.tree.getDisplayItems(value.tree.currentLeafId),
              6,
            );
          }
        } catch (error) {
          detail.treeDisplayItemsError = String(error?.name || error);
        }
        try {
          if (typeof value.tree.getDisplayTurns === 'function') {
            detail.treeDisplayTurns = collectionSummary(
              value.tree.getDisplayTurns(value.tree.currentLeafId),
              6,
            );
          }
        } catch (error) {
          detail.treeDisplayTurnsError = String(error?.name || error);
        }
      }
    } catch (error) {
      detail.treeError = String(error?.name || error);
    }
    try {
      if (value.data && typeof value.data === 'object') {
        detail.dataShape = valueShape(value.data, 1);
        detail.dataOwnValueShapes = ownValueShapes(value.data, 16);
      }
    } catch (error) {
      detail.dataError = String(error?.name || error);
    }
    return detail;
  }

  function isConversationWrapper(value) {
    return !!value &&
      typeof value === 'object' &&
      typeof value.serverId$ === 'function' &&
      typeof value.id === 'string' &&
      value.ctx &&
      typeof value.ctx === 'object' &&
      value.config &&
      typeof value.config === 'object';
  }

  function conversationObjectLabel(value) {
    if (!value || typeof value !== 'object') return '';
    try {
      let label = conversationObjectIds.get(value);
      if (!label) {
        label = `conversation:${++nextConversationObjectId}`;
        conversationObjectIds.set(value, label);
      }
      return label;
    } catch {
      return '';
    }
  }

  function conversationIdentityShape(value) {
    if (!isConversationWrapper(value)) return valueShape(value, 1);
    const shape = {
      kind: 'conversation-wrapper',
      object: conversationObjectLabel(value),
      ownKeys: ownKeySummary(value, 12),
      ctxKeys: objectKeys(value.ctx, 12),
      configKeys: objectKeys(value.config, 12),
    };
    let serverId = '';
    try {
      serverId = String(value.serverId$() || '');
    } catch (error) {
      shape.serverIdError = String(error?.name || error);
    }
    shape.id = idShape(String(value.id || ''));
    shape.serverId = idShape(serverId);
    shape.idMatchesServerId = !!serverId && String(value.id || '') === serverId;
    return shape;
  }

  function reactElementTypeName(type) {
    try {
      if (typeof type === 'string') return type;
      if (typeof type === 'function') return type.displayName || type.name || '';
      if (type && typeof type === 'object') {
        return type.displayName ||
          type.name ||
          type.render?.displayName ||
          type.render?.name ||
          type.type?.displayName ||
          type.type?.name ||
          '';
      }
    } catch {}
    return '';
  }

  function identityThreadPropShapes(props) {
    const selected = {};
    if (!props || typeof props !== 'object') return selected;
    for (const key of [
      'conversationId',
      'urlThreadId',
      'clientThreadId',
      'serverThreadId',
      'threadId',
      'turnId',
      'forceRenderedTurnId',
    ]) {
      try {
        if (Object.prototype.hasOwnProperty.call(props, key)) {
          selected[key] = scalarValueShape(props[key], key);
        }
      } catch {}
    }
    return selected;
  }

  function pushConversationIdentityRecord(records, record) {
    if (!record || records.length >= maxConversationIdentityRecords) return;
    records.push(record);
  }

  function recordConversationIdentityFromProps(records, source, depth, name, tag, props, path) {
    if (!props || typeof props !== 'object' || records.length >= maxConversationIdentityRecords) return;
    let conversation = null;
    try {
      conversation = props.conversation;
    } catch {}
    const hasConversation = isConversationWrapper(conversation);
    const selectedThreadProps = identityThreadPropShapes(props);
    const hasSelectedThreadProps = Object.keys(selectedThreadProps).length > 0;
    if (!hasConversation && !hasSelectedThreadProps && !/^(KFe|JFe|XFe|Sbe|Zqn|qJn)$/.test(name)) {
      return;
    }
    const record = {
      source,
      depth,
      name,
      tag,
      path,
      selectedThreadProps,
    };
    if (hasConversation) {
      record.conversation = conversationIdentityShape(conversation);
    }
    pushConversationIdentityRecord(records, record);
  }

  function collectReactElementConversationIdentities(value, source, path, depth, records, seen) {
    if (depth > 5 || records.length >= maxConversationIdentityRecords) return;
    if (Array.isArray(value)) {
      for (let index = 0; index < value.length && records.length < maxConversationIdentityRecords; index += 1) {
        collectReactElementConversationIdentities(
          value[index],
          source,
          `${path}[${index}]`,
          depth + 1,
          records,
          seen,
        );
      }
      return;
    }
    if (!value || typeof value !== 'object') return;
    if (seen.has(value)) return;
    seen.add(value);
    const maybeElement = Object.prototype.hasOwnProperty.call(value, '$$typeof') &&
      Object.prototype.hasOwnProperty.call(value, 'type') &&
      Object.prototype.hasOwnProperty.call(value, 'props');
    if (!maybeElement) return;
    const name = reactElementTypeName(value.type);
    const props = value.props;
    recordConversationIdentityFromProps(records, `${source}:element`, depth, name, '', props, path);
    try {
      if (props && typeof props === 'object') {
        if (Object.prototype.hasOwnProperty.call(props, 'children')) {
          collectReactElementConversationIdentities(
            props.children,
            source,
            `${path}>children`,
            depth + 1,
            records,
            seen,
          );
        }
        if (Object.prototype.hasOwnProperty.call(props, 'fallback')) {
          collectReactElementConversationIdentities(
            props.fallback,
            source,
            `${path}>fallback`,
            depth + 1,
            records,
            seen,
          );
        }
      }
    } catch {}
  }

  function conversationIdentitySnapshot(root, reason = 'snapshot') {
    const records = [];
    const seenFibers = new Set();
    const seenElements = new WeakSet();
    const stack = [];
    let threadFiber = null;
    try {
      const rootFiber = root?.current || root || lastReactRoot?.current || lastReactRoot || null;
      if (rootFiber) stack.push({fiber: rootFiber, depth: 0, source: 'root'});
    } catch {}
    try {
      threadFiber = fiberForDomNode(document.getElementById('thread'));
      if (threadFiber) stack.push({fiber: threadFiber, depth: 0, source: 'thread'});
    } catch {}
    while (stack.length && records.length < maxConversationIdentityRecords && seenFibers.size < 8000) {
      const {fiber, depth, source} = stack.pop();
      if (!fiber || seenFibers.has(fiber)) continue;
      seenFibers.add(fiber);
      let name = '';
      let props = null;
      let tag = 0;
      try {
        name = reactFiberName(fiber);
        props = fiber.memoizedProps;
        tag = Number(fiber.tag);
      } catch {}
      const path = fiberPathSummary(fiber, 8).map((item) => item.name || item.tagLabel).join('>');
      recordConversationIdentityFromProps(records, source, depth, name, tag, props, path);
      try {
        if (tag === 13 && props && typeof props === 'object') {
          collectReactElementConversationIdentities(
            props.children,
            `${source}:suspense`,
            `${path}>children`,
            0,
            records,
            seenElements,
          );
        }
      } catch {}
      try {
        if (fiber.sibling) stack.push({fiber: fiber.sibling, depth, source});
        if (fiber.child) stack.push({fiber: fiber.child, depth: depth + 1, source});
      } catch {}
    }
    const identities = records
      .map((record) => {
        const conversation = record.conversation || {};
        return [
          record.source,
          record.name,
          conversation.object || '',
          conversation.id || '',
          conversation.serverId || '',
          String(!!conversation.idMatchesServerId),
        ].join('/');
      })
      .join('|');
    return {
      reason,
      url: compactUrl(location.href),
      threadFiberPresent: !!threadFiber,
      visited: seenFibers.size,
      recordCount: records.length,
      signature: identities.slice(0, 2000),
      records,
    };
  }

  function recordConversationIdentitySnapshot(root, reason) {
    let sample = null;
    try {
      sample = conversationIdentitySnapshot(root, reason);
      const changed = sample.signature !== lastConversationIdentitySignature;
      if (changed) {
        lastConversationIdentitySignature = sample.signature;
      }
      pushConversationIdentitySample(sample, {changed});
      if (changed || reason !== 'commit') {
        record('conversation-identity', {
          reason,
          changed,
          recordCount: sample.recordCount,
          records: sample.records.slice(0, 8),
        });
      }
    } catch (error) {
      record('conversation-identity-error', {
        reason,
        name: String(error?.name || ''),
        message: String(error?.message || error).slice(0, 240),
      });
    }
    return sample;
  }

  function conversationIdentityTraceState() {
    const current = recordConversationIdentitySnapshot(null, 'snapshot');
    return {
      ...conversationIdentityStats,
      now: nowMs(),
      current,
      samples: conversationIdentitySamples.slice(-24),
    };
  }

  function shouldDeepProbeConversationWrapper(value) {
    if (!isConversationWrapper(value)) return false;
    if (deepProbeCount >= 24) return false;
    try {
      const count = deepProbeCounts.get(value) || 0;
      if (count >= 3) return false;
      deepProbeCounts.set(value, count + 1);
      deepProbeCount += 1;
      return true;
    } catch {
      return false;
    }
  }

  function zeroArgFunctionResultShapes(value, limit = 16, depth = 0) {
    const shapes = [];
    if (!value || typeof value !== 'object') return shapes;
    const startedAt = nowMs();
    try {
      const keys = Reflect.ownKeys(value);
      for (let index = 0; index < keys.length && shapes.length < limit; index += 1) {
        if (nowMs() - startedAt > 150) {
          shapes.push({truncated: true, reason: 'time-budget'});
          break;
        }
        const key = keys[index];
        const descriptor = Object.getOwnPropertyDescriptor(value, key);
        if (!descriptor || !('value' in descriptor)) continue;
        const fn = descriptor.value;
        if (typeof fn !== 'function' || fn.length !== 0) continue;
        const entry = {key: propKeyLabel(key), index, name: String(fn.name || '').slice(0, 80)};
        try {
          const result = fn.call(value);
          entry.resultShape = valueShape(result, 1);
          const detail = depth < 2 ? conversationResultDetail(result, depth + 1) : null;
          if (detail) entry.resultDetail = detail;
        } catch (error) {
          entry.error = String(error?.name || error);
        }
        shapes.push(entry);
      }
    } catch (error) {
      shapes.push({error: String(error?.name || error)});
    }
    return shapes;
  }

  function prototypeMethodSummary(value, limit = 24) {
    const methods = [];
    if (!value || typeof value !== 'object') return methods;
    const seen = new Set();
    try {
      let proto = Object.getPrototypeOf(value);
      let depth = 0;
      while (proto && proto !== Object.prototype && depth < 4 && methods.length < limit) {
        for (const key of Reflect.ownKeys(proto)) {
          if (methods.length >= limit) break;
          const label = propKeyLabel(key);
          if (label === 'constructor' || seen.has(label)) continue;
          seen.add(label);
          try {
            const descriptor = Object.getOwnPropertyDescriptor(proto, key);
            if (!descriptor) continue;
            const valueKind = 'value' in descriptor ? typeof descriptor.value : '';
            const accessorKind = 'get' in descriptor && descriptor.get ? 'getter' : '';
            if (valueKind === 'function' || accessorKind) {
              methods.push({
                key: label,
                kind: valueKind === 'function' ? 'method' : accessorKind,
                name: valueKind === 'function' ? String(descriptor.value.name || '').slice(0, 80) : '',
                length: valueKind === 'function' ? Number(descriptor.value.length) || 0 : 0,
              });
            }
          } catch (error) {
            methods.push({key: label, error: String(error?.name || error)});
          }
        }
        proto = Object.getPrototypeOf(proto);
        depth += 1;
      }
    } catch (error) {
      methods.push({error: String(error?.name || error)});
    }
    return methods;
  }

  function valueProbe(value) {
    const result = {kind: shapeKind(value)};
    try { result.tag = Object.prototype.toString.call(value); } catch (error) { result.tagError = String(error?.name || error); }
    try { result.constructorName = value?.constructor?.name || ''; } catch (error) { result.constructorError = String(error?.name || error); }
    try { result.keys = value && typeof value === 'object' ? Object.keys(value).slice(0, 20) : []; } catch (error) { result.keysError = String(error?.name || error); }
    try {
      result.ownKeys = value && typeof value === 'object'
        ? Reflect.ownKeys(value).slice(0, 20).map((key) => typeof key === 'symbol' ? `symbol:${String(key.description || '')}` : String(key))
        : [];
    } catch (error) {
      result.ownKeysError = String(error?.name || error);
    }
    try { result.thenType = typeof value?.then; } catch {}
    try { result.mapLikeSize = mapLikeSize(value); } catch {}
    const childShapes = {};
    for (const key of result.keys || []) {
      try {
        childShapes[key] = shapeKind(value[key]);
      } catch (error) {
        childShapes[key] = `error:${String(error?.name || error)}`;
      }
    }
    result.childShapes = childShapes;
    return result;
  }

  function reactRouterState() {
    const router = window.__reactRouterDataRouter;
    const state = router?.state;
    if (!state || typeof state !== 'object') return {present: !!router};
    const loaderDataKeys = objectKeys(state.loaderData, 40);
    const actionDataKeys = objectKeys(state.actionData, 40);
    const errorKeys = objectKeys(state.errors, 40);
    const matches = Array.isArray(state.matches)
      ? state.matches.slice(-12).map((match) => ({
          id: String(match?.id || '').slice(0, 160),
          pathname: String(match?.pathname || match?.pathnameBase || '').slice(0, 160),
          hasLoaderData: loaderDataKeys.includes(String(match?.id || '')),
        }))
      : [];
    const loaderDataShapes = {};
    const loaderDataProbes = {};
    try {
      for (const key of loaderDataKeys.slice(0, 10)) {
        loaderDataShapes[key] = valueShape(state.loaderData?.[key]);
        loaderDataProbes[key] = valueProbe(state.loaderData?.[key]);
      }
    } catch {}
    return {
      present: true,
      locationPathname: String(state.location?.pathname || ''),
      navigationState: String(state.navigation?.state || ''),
      revalidation: String(state.revalidation || ''),
      loaderDataKeys,
      loaderDataShapes,
      loaderDataProbes,
      actionDataKeys,
      errorKeys,
      fetcherCount: mapLikeSize(state.fetchers),
      blockerCount: mapLikeSize(state.blockers),
      matches,
    };
  }

  function reactQueryState() {
    const cache = window.__REACT_QUERY_CACHE__;
    if (!cache) return {present: false};
    let queries = [];
    try {
      if (typeof cache.getAll === 'function') {
        queries = cache.getAll();
      } else if (Array.isArray(cache.queries)) {
        queries = cache.queries;
      } else if (Array.isArray(cache)) {
        queries = cache;
      }
    } catch {}
    const queryStates = queries
      .slice(0, 80)
      .map((query) => query?.state || query)
      .filter((state) => state && typeof state === 'object');
    return {
      present: true,
      kind: Object.prototype.toString.call(cache),
      keys: objectKeys(cache, 16),
      queryCount: queries.length,
      statusCounts: countBy(queryStates.map((state) => state.status)),
      fetchStatusCounts: countBy(queryStates.map((state) => state.fetchStatus)),
    };
  }

  function routeModulesState() {
    const modules = window.__reactRouterRouteModules;
    if (!modules || typeof modules !== 'object') return {present: !!modules};
    const keys = objectKeys(modules, 80);
    const selected = {};
    for (const key of keys.filter((key) => /conversation|root/i.test(key)).slice(0, 12)) {
      const module = modules[key];
      const exportShapes = {};
      for (const exportName of objectKeys(module, 60)) {
        const value = module?.[exportName];
        if (
          exportName === 'default' ||
          /loader|action|component|ErrorBoundary|HydrateFallback|shouldRevalidate/i.test(exportName)
        ) {
          exportShapes[exportName] = {
            type: typeof value,
            name: typeof value === 'function' ? String(value.name || '') : '',
            keys: value && typeof value === 'object' ? objectKeys(value, 20) : [],
          };
        }
      }
      selected[key] = {
        keys: objectKeys(module, 60),
        ownKeys: ownKeySummary(module, 60),
        exportShapes,
      };
    }
    return {
      present: true,
      kind: Object.prototype.toString.call(modules),
      keys,
      selected,
    };
  }

  function appRuntimeState() {
    const windowKeys = [];
    try {
      for (const key of Object.keys(window)) {
        if (/react|router|remix|next/i.test(key)) windowKeys.push(key);
        if (windowKeys.length >= 20) break;
      }
    } catch {}
    return {
      windowKeys,
      router: reactRouterState(),
      routeModules: routeModulesState(),
      queryCache: reactQueryState(),
      nextData: !!document.querySelector('script#__NEXT_DATA__'),
      serializedAppScripts: document.querySelectorAll('script[type="application/json"], script[data-flight], script[data-rsc]').length,
      platformTaskApis: {
        requestIdleCallback: typeof window.requestIdleCallback,
        cancelIdleCallback: typeof window.cancelIdleCallback,
        scheduler: typeof window.scheduler,
        schedulerPostTask: typeof window.scheduler?.postTask,
        MessageChannel: typeof window.MessageChannel,
        ResizeObserver: typeof window.ResizeObserver,
        IntersectionObserver: typeof window.IntersectionObserver,
      },
      observerApis: observerApiState(),
      reactCommits: reactCommitState(),
      activeElement: document.activeElement
        ? {
            tag: document.activeElement.tagName,
            id: document.activeElement.id || '',
            role: document.activeElement.getAttribute?.('role') || '',
            testid: document.activeElement.getAttribute?.('data-testid') || '',
          }
        : null,
    };
  }

  function recordAppState(reason) {
    record('app-state', {reason, state: appRuntimeState()});
  }

  function eventLoopState() {
    return {...eventLoopStats, now: nowMs()};
  }

  function messageTaskState() {
    return {...messageTaskStats, now: nowMs()};
  }

  function observerApiState() {
    return {...observerApiStats, now: nowMs()};
  }

  function domMutationState() {
    return {...domMutationStats, now: nowMs(), samples: domMutationSamples.slice(-40)};
  }

  function reactCommitState() {
    return {
      ...reactCommitStats,
      now: nowMs(),
      samples: reactFiberSamples.slice(-40),
    };
  }

  function reactFiberName(fiber) {
    try {
      const type = fiber?.elementType || fiber?.type;
      if (typeof type === 'string') return type;
      if (typeof type === 'function') return type.displayName || type.name || '';
      if (type && typeof type === 'object') {
        return type.displayName ||
          type.name ||
          type.render?.displayName ||
          type.render?.name ||
          type.type?.displayName ||
          type.type?.name ||
          '';
      }
    } catch {}
    return '';
  }

  function reactFiberSourceHint(fiber) {
    try {
      const type = fiber?.type || fiber?.elementType;
      const fn = typeof type === 'function' ? type :
        typeof type?.render === 'function' ? type.render :
        typeof type?.type === 'function' ? type.type : null;
      if (!fn) return '';
      return Function.prototype.toString.call(fn).replace(/\s+/g, ' ').slice(0, 2200);
    } catch {}
    return '';
  }

  function isConversationThreadListFiber(name, props) {
    return name === 's' &&
      !!props &&
      typeof props === 'object' &&
      Object.prototype.hasOwnProperty.call(props, 'conversation') &&
      Object.prototype.hasOwnProperty.call(props, 'onRequestCompletion') &&
      Object.prototype.hasOwnProperty.call(props, 'scrollContainerRef') &&
      Object.prototype.hasOwnProperty.call(props, 'disableScrollToMessage');
  }

  function shouldRecordFiberSourceHint(name, props = null) {
    return /^(NFe|BFe|e8|T2|qDr|\$Ar|XFe|pY|iyr|Sbe|KFe|JFe|nFe|uje|nJn|eje|Zqn|qJn|d8|u4n|T9|gQe)$/.test(name) ||
      isConversationThreadListFiber(name, props);
  }

  function propHintString(value) {
    if (typeof value !== 'string') return '';
    return value.length > 160 ? `${value.slice(0, 160)}...` : value;
  }

  function scalarValueShape(value, key = '') {
    const detail = valueShape(value);
    if (typeof value === 'boolean' || typeof value === 'number') {
      detail.scalar = value;
    } else if (typeof value === 'string') {
      detail.scalar = safeScalarToken(value);
      if (/id|thread|conversation|turn|message/i.test(String(key || ''))) {
        detail.id = idShape(value);
      }
    }
    return detail;
  }

  function selectedPropValueDetail(value, options = {}) {
    const key = String(options.key || '');
    const detail = scalarValueShape(value, key);
    if (typeof value === 'boolean' || typeof value === 'number' || typeof value === 'string') return detail;
    if (typeof value === 'function' && options.callZeroArg && value.length === 0) {
      try {
        detail.zeroArgResult = scalarValueShape(value(), key);
      } catch (error) {
        detail.zeroArgError = String(error?.name || error);
      }
      return detail;
    }
    if (!value || typeof value !== 'object') return detail;
    try {
      if (typeof value.id === 'string') detail.id = idShape(value.id);
    } catch {}
    try {
      if (typeof value.serverId$ === 'function') {
        detail.serverId = idShape(String(value.serverId$() || ''));
      }
    } catch (error) {
      detail.serverIdError = String(error?.name || error);
    }
    try {
      if (value.ctx && typeof value.ctx === 'object') {
        detail.ctxKeys = objectKeys(value.ctx, 12);
      }
    } catch {}
    try {
      if (value.config && typeof value.config === 'object') {
        detail.configKeys = objectKeys(value.config, 12);
      }
    } catch {}
    try {
      detail.ownValueShapes = ownValueShapes(value, 16);
    } catch {}
    try {
      detail.prototypeMethods = prototypeMethodSummary(value, 24);
    } catch {}
    if (
      (options.forceDeepConversationProbe && isConversationWrapper(value)) ||
      shouldDeepProbeConversationWrapper(value)
    ) {
      try {
        detail.zeroArgFunctionResults = zeroArgFunctionResultShapes(value, 16);
      } catch {}
      try {
        detail.ctxOwnValueShapes = ownValueShapes(value.ctx, 16);
      } catch {}
      try {
        detail.ctxStoreLike = storeLikeSummaries(value.ctx, 8);
      } catch {}
      try {
        detail.ctxZeroArgFunctionResults = zeroArgFunctionResultShapes(value.ctx, 16);
      } catch {}
    }
    return detail;
  }

  function reactPropsSummary(props) {
    if (!props || typeof props !== 'object') return {kind: typeof props};
    const keys = objectKeys(props, 30);
    const className = typeof props.className === 'string' ? props.className : '';
    const children = props.children;
    const selectedValueShapes = {};
    for (const key of [
      'conversation',
      'conversationId',
      'urlThreadId',
      'thread',
      'threadId',
      'serverThreadId',
      'clientThreadId',
      'turn',
      'turnId',
      'turnIndex',
      'message',
      'messages',
      'displayItems',
      'isThreadContentLoading',
      'isNewThread',
      'when$',
      'fallback',
      'renderEmptyState',
      'renderEmptyFooter',
      'hideComposer',
      'isComposerPinnedToBottomOnEmptyState',
      'isScrolledFromBottom$',
      'shouldUseUnifiedComposer',
      'isCompletionInProgress',
      'isGizmoThread',
      'isProjectThread',
      'layoutMode',
      'currentModelId',
      'pageLoadSearchQuery',
      'forceRenderedTurnId',
      'children',
    ]) {
      if (Object.prototype.hasOwnProperty.call(props, key)) {
        selectedValueShapes[key] = selectedPropValueDetail(props[key], {
          key,
          callZeroArg: key.endsWith('$') || key === 'when$',
        });
      }
    }
    return {
      kind: 'object',
      keys,
      id: propHintString(props.id),
      role: propHintString(props.role),
      testid: propHintString(props['data-testid']),
      ariaHidden: propHintString(props['aria-hidden']),
      inert: !!props.inert,
      dataTurn: propHintString(props['data-turn']),
      dataMessageId: props['data-message-id'] ? idShape(String(props['data-message-id'])) : undefined,
      dataMessageAuthorRole: propHintString(props['data-message-author-role']),
      classHints: className
        .split(/\s+/)
        .filter((name) => /thread|conversation|message|turn|composer|markdown|viewport|virtual|list/i.test(name))
        .slice(0, 8),
      childKind: Array.isArray(children) ? `array:${children.length}` : typeof children,
      textChildLen: typeof children === 'string' ? children.length : 0,
      hasDangerousHtml: !!props.dangerouslySetInnerHTML,
      selectedValueShapes,
    };
  }

  function reactFiberHints(fiber, name, props) {
    const hints = [];
    const haystack = [
      name,
      props?.id,
      props?.role,
      props?.className,
      props?.['data-testid'],
      props?.['data-turn'],
      props?.['data-message-id'],
      props?.['data-message-author-role'],
    ]
      .filter((value) => typeof value === 'string')
      .join(' ');
    if (/conversation/i.test(haystack)) hints.push('conversation');
    if (/thread/i.test(haystack)) hints.push('thread');
    if (/message/i.test(haystack)) hints.push('message');
    if (/turn/i.test(haystack)) hints.push('turn');
    if (/markdown/i.test(haystack)) hints.push('markdown');
    if (/composer|prompt/i.test(haystack)) hints.push('composer');
    if (props && typeof props === 'object') {
      if (Object.prototype.hasOwnProperty.call(props, 'conversation')) hints.push('prop:conversation');
      if (Object.prototype.hasOwnProperty.call(props, 'conversationId')) hints.push('prop:conversationId');
      if (Object.prototype.hasOwnProperty.call(props, 'threadId')) hints.push('prop:threadId');
      if (Object.prototype.hasOwnProperty.call(props, 'turn')) hints.push('prop:turn');
      if (Object.prototype.hasOwnProperty.call(props, 'message')) hints.push('prop:message');
      if (Object.prototype.hasOwnProperty.call(props, 'messages')) hints.push('prop:messages');
    }
    return hints;
  }

  function summarizeReactCommitRoot(root) {
    const summary = {
      visited: 0,
      hostComponents: 0,
      hostText: 0,
      conversationHints: 0,
      messageHints: 0,
      turnHints: 0,
      markdownHints: 0,
      composerHints: 0,
      dataTurnProps: 0,
      dataMessageProps: 0,
      dataRoleProps: 0,
      textFiberLenMax: 0,
      samples: [],
    };
    const stack = [];
    try {
      if (root?.current) stack.push(root.current);
      else if (root) stack.push(root);
    } catch {}
    const seen = new Set();
    while (stack.length && summary.visited < 5000) {
      const fiber = stack.pop();
      if (!fiber || seen.has(fiber)) continue;
      seen.add(fiber);
      summary.visited += 1;
      const props = fiber.memoizedProps;
      const name = reactFiberName(fiber);
      const tag = Number(fiber.tag);
      if (tag === 5) summary.hostComponents += 1;
      if (tag === 6) {
        summary.hostText += 1;
        const textLen = String(fiber.memoizedProps || '').length;
        if (textLen > summary.textFiberLenMax) summary.textFiberLenMax = textLen;
      }
      if (props && typeof props === 'object') {
        if (props['data-turn'] != null) summary.dataTurnProps += 1;
        if (props['data-message-id'] != null) summary.dataMessageProps += 1;
        if (props['data-message-author-role'] != null) summary.dataRoleProps += 1;
      }
      const hints = reactFiberHints(fiber, name, props);
      if (hints.includes('conversation')) summary.conversationHints += 1;
      if (hints.includes('message')) summary.messageHints += 1;
      if (hints.includes('turn')) summary.turnHints += 1;
      if (hints.includes('markdown')) summary.markdownHints += 1;
      if (hints.includes('composer')) summary.composerHints += 1;
      if (hints.length && summary.samples.length < 16) {
        summary.samples.push({
          name,
          tag,
          hints,
          props: reactPropsSummary(props),
          stateNode: fiber.stateNode?.nodeType === Node.ELEMENT_NODE ? elementBrief(fiber.stateNode) : null,
        });
      }
      if (fiber.sibling) stack.push(fiber.sibling);
      if (fiber.child) stack.push(fiber.child);
    }
    return summary;
  }

  function currentConversationWrapperSnapshots(limit = 4) {
    const snapshots = [];
    const stack = [];
    const seenFibers = new Set();
    const seenConversations = new WeakSet();
    try {
      if (lastReactRoot?.current) stack.push(lastReactRoot.current);
      else if (lastReactRoot) stack.push(lastReactRoot);
    } catch {}
    while (stack.length && snapshots.length < limit && seenFibers.size < 5000) {
      const fiber = stack.pop();
      if (!fiber || seenFibers.has(fiber)) continue;
      seenFibers.add(fiber);
      try {
        const props = fiber.memoizedProps;
        const conversation = props?.conversation;
        if (
          isConversationWrapper(conversation) &&
          !seenConversations.has(conversation)
        ) {
          seenConversations.add(conversation);
          snapshots.push({
            name: reactFiberName(fiber),
            tag: Number(fiber.tag),
            hints: reactFiberHints(fiber, reactFiberName(fiber), props),
            conversation: selectedPropValueDetail(conversation, {forceDeepConversationProbe: true}),
            subtree: conversationSubtreeSummary(fiber, 160),
          });
        }
      } catch (error) {
        snapshots.push({
          error: String(error?.name || error),
          message: String(error?.message || error).slice(0, 160),
        });
      }
      try {
        if (fiber.sibling) stack.push(fiber.sibling);
        if (fiber.child) stack.push(fiber.child);
      } catch {}
    }
    return {
      rootPresent: !!lastReactRoot,
      visited: seenFibers.size,
      count: snapshots.length,
      snapshots,
    };
  }

  function collectionItems(value, limit = 8) {
    const items = [];
    if (!value) return items;
    try {
      if (Array.isArray(value)) {
        return value.slice(0, limit);
      }
      if (value instanceof Map) {
        for (const item of value.values()) {
          if (items.length >= limit) break;
          items.push(item);
        }
        return items;
      }
      if (value instanceof Set) {
        for (const item of value.values()) {
          if (items.length >= limit) break;
          items.push(item);
        }
        return items;
      }
      if (typeof value === 'object') {
        for (const key of Object.keys(value)) {
          if (items.length >= limit) break;
          items.push(value[key]);
        }
      }
    } catch {}
    return items;
  }

  function turnMaterializationSummary(turn) {
    const summary = valueShape(turn, 1);
    if (!turn || typeof turn !== 'object') return summary;
    try {
      if (typeof turn.id === 'string') summary.id = idShape(turn.id);
    } catch {}
    try {
      if (typeof turn.role === 'string') summary.role = turn.role;
    } catch {}
    try {
      const messages = Array.isArray(turn.messages) ? turn.messages : [];
      summary.messageCount = messages.length;
      summary.messageRoles = countBy(messages.slice(0, 12).map((message) => {
        try {
          return message?.author?.role || message?.author?.type || '';
        } catch {
          return '';
        }
      }));
      summary.messageStatuses = countBy(messages.slice(0, 12).map((message) => {
        try {
          return message?.status || '';
        } catch {
          return '';
        }
      }));
    } catch {}
    try {
      if (Array.isArray(turn.messageGroups)) {
        summary.messageGroupCount = turn.messageGroups.length;
        summary.messageGroupTypes = countBy(
          turn.messageGroups.slice(0, 12).map((group) => group?.type),
        );
      }
    } catch {}
    return summary;
  }

  function displayItemMaterializationSummary(item) {
    const summary = valueShape(item, 1);
    if (!item || typeof item !== 'object') return summary;
    try {
      if (typeof item.type === 'string') summary.type = item.type;
    } catch {}
    try {
      if (typeof item.id === 'string') summary.id = idShape(item.id);
    } catch {}
    try {
      if (item.turn) summary.turn = turnMaterializationSummary(item.turn);
    } catch {}
    return summary;
  }

  function conversationTreeMaterializationSummary(value) {
    if (!value || typeof value !== 'object' || !value.tree || typeof value.tree !== 'object') {
      return null;
    }
    const tree = value.tree;
    const summary = {
      resultShape: valueShape(value, 1),
      resultFields: semanticScalarFields(value),
      treeShape: valueShape(tree, 1),
    };
    try {
      if (typeof value.version === 'number') summary.version = value.version;
      if (typeof value._treeVersion === 'number') summary.treeVersion = value._treeVersion;
      if (typeof value.isLoading === 'boolean') summary.isLoading = value.isLoading;
    } catch {}
    let currentLeafId = '';
    try {
      currentLeafId = String(tree.currentLeafId || '');
      summary.currentLeafId = idShape(currentLeafId);
    } catch (error) {
      summary.currentLeafIdError = String(error?.name || error);
    }
    try {
      summary.nodeCount = mapLikeSize(tree.nodes);
    } catch (error) {
      summary.nodeCountError = String(error?.name || error);
    }
    try {
      if (typeof tree.getDisplayTurns === 'function') {
        const turns = tree.getDisplayTurns(currentLeafId);
        summary.displayTurnCount = mapLikeSize(turns);
        summary.displayTurnRoles = collectionItems(turns, 12).map((turn) => {
          try {
            return String(turn?.role || '');
          } catch {
            return '';
          }
        });
        summary.displayTurns = collectionItems(turns, 6).map(turnMaterializationSummary);
      }
    } catch (error) {
      summary.displayTurnsError = String(error?.name || error);
    }
    try {
      if (typeof tree.getDisplayItems === 'function') {
        const items = tree.getDisplayItems(currentLeafId);
        summary.displayItemCount = mapLikeSize(items);
        summary.displayItemTypes = collectionItems(items, 12).map((item) => {
          try {
            return String(item?.type || '');
          } catch {
            return '';
          }
        });
        summary.displayItems = collectionItems(items, 6).map(displayItemMaterializationSummary);
      }
    } catch (error) {
      summary.displayItemsError = String(error?.name || error);
    }
    return summary;
  }

  function conversationWrapperMaterializationProbe(limit = 4, options = {}) {
    const includeSubtree = options.includeSubtree !== false;
    const maxFibers = Number(options.maxFibers || 9000);
    const probes = [];
    const stack = [];
    const seenFibers = new Set();
    const seenConversations = new WeakSet();
    try {
      const rootFiber = lastReactRoot?.current || lastReactRoot || null;
      if (rootFiber) stack.push({fiber: rootFiber, depth: 0, source: 'root'});
    } catch {}
    try {
      const threadFiber = fiberForDomNode(document.getElementById('thread'));
      if (threadFiber) stack.push({fiber: threadFiber, depth: 0, source: 'thread'});
    } catch {}
    while (stack.length && probes.length < limit && seenFibers.size < maxFibers) {
      const {fiber, depth, source} = stack.pop();
      if (!fiber || seenFibers.has(fiber)) continue;
      seenFibers.add(fiber);
      try {
        const props = fiber.memoizedProps;
        const conversation = props?.conversation;
        if (isConversationWrapper(conversation) && !seenConversations.has(conversation)) {
          seenConversations.add(conversation);
          const zeroArgResults = [];
          const keys = Reflect.ownKeys(conversation);
          for (let index = 0; index < keys.length && zeroArgResults.length < 16; index += 1) {
            const key = keys[index];
            let descriptor = null;
            try {
              descriptor = Object.getOwnPropertyDescriptor(conversation, key);
            } catch {}
            const fn = descriptor && 'value' in descriptor ? descriptor.value : null;
            if (typeof fn !== 'function' || fn.length !== 0) continue;
            const entry = {index, key: propKeyLabel(key), name: String(fn.name || '').slice(0, 80)};
            try {
              const result = fn.call(conversation);
              entry.resultShape = valueShape(result, 1);
              const materialized = conversationTreeMaterializationSummary(result);
              if (materialized) entry.materializedTree = materialized;
            } catch (error) {
              entry.error = String(error?.name || error);
            }
            zeroArgResults.push(entry);
          }
          probes.push({
            source,
            depth,
            name: reactFiberName(fiber),
            tag: Number(fiber.tag),
            tagLabel: reactFiberTagLabel(Number(fiber.tag)),
            hints: reactFiberHints(fiber, reactFiberName(fiber), props),
            path: fiberPathSummary(fiber, 10),
            conversation: conversationIdentityShape(conversation),
            zeroArgResults,
            subtree: includeSubtree ? conversationSubtreeSummary(fiber, 80) : undefined,
          });
        }
      } catch (error) {
        probes.push({
          source,
          depth,
          error: String(error?.name || error),
          message: String(error?.message || error).slice(0, 160),
        });
      }
      try {
        if (fiber.sibling) stack.push({fiber: fiber.sibling, depth, source});
        if (fiber.child) stack.push({fiber: fiber.child, depth: depth + 1, source});
      } catch {}
    }
    return {
      rootPresent: !!lastReactRoot,
      visited: seenFibers.size,
      count: probes.length,
      probes,
    };
  }

  function conversationMaterializationSignature(probe) {
    if (!probe || !Array.isArray(probe.probes)) return '';
    return probe.probes.map((item) => {
      const conversation = item.conversation || {};
      const trees = [];
      for (const result of item.zeroArgResults || []) {
        const tree = result.materializedTree;
        if (!tree) continue;
        trees.push([
          result.index,
          tree.version ?? '',
          tree.treeVersion ?? '',
          tree.isLoading ?? '',
          tree.displayTurnCount ?? '',
          (tree.displayTurnRoles || []).join(','),
          tree.displayItemCount ?? '',
        ].join(':'));
      }
      return [
        item.source,
        item.name,
        conversation.object || '',
        conversation.id || '',
        conversation.serverId || '',
        String(!!conversation.idMatchesServerId),
        trees.join(';'),
      ].join('/');
    }).join('|').slice(0, 2400);
  }

  function recordConversationMaterializationSnapshot(reason, options = {}) {
    let sample = null;
    try {
      sample = conversationWrapperMaterializationProbe(2, {
        includeSubtree: options.includeSubtree === true,
        maxFibers: options.maxFibers || 1200,
      });
      sample.reason = reason;
      sample.url = compactUrl(location.href);
      sample.signature = conversationMaterializationSignature(sample);
      const changed = sample.signature !== lastConversationMaterializationSignature;
      if (changed) {
        lastConversationMaterializationSignature = sample.signature;
      }
      pushConversationMaterializationSample(sample, {changed});
      if (changed || reason !== 'commit') {
        record('conversation-materialization', {
          reason,
          changed,
          count: sample.count,
          probes: sample.probes.slice(0, 2).map((probe) => ({
            source: probe.source,
            name: probe.name,
            conversation: probe.conversation,
            zeroArgResults: (probe.zeroArgResults || [])
              .filter((item) => item.materializedTree)
              .slice(0, 4)
              .map((item) => ({
                index: item.index,
                key: item.key,
                materializedTree: item.materializedTree,
              })),
          })),
        });
      }
    } catch (error) {
      record('conversation-materialization-error', {
        reason,
        name: String(error?.name || ''),
        message: String(error?.message || error).slice(0, 240),
      });
    }
    return sample;
  }

  function conversationMaterializationTraceState() {
    const current = recordConversationMaterializationSnapshot('snapshot', {
      includeSubtree: true,
      maxFibers: 9000,
    });
    return {
      ...conversationMaterializationStats,
      now: nowMs(),
      current,
      samples: conversationMaterializationSamples.slice(-24),
    };
  }

  function compactSelectedPropValue(key, value) {
    const detail = selectedPropValueDetail(value, {
      key,
      callZeroArg: String(key).endsWith('$') || key === 'when$',
    });
    try {
      if (typeof value === 'string' && /id|thread|conversation|turn|message/i.test(key)) {
        detail.id = idShape(value);
      }
    } catch {}
    return detail;
  }

  function compactFiberPropsForTrace(props) {
    if (!props || typeof props !== 'object') return {kind: typeof props};
    const selected = {};
    for (const key of [
      'conversation',
      'conversationId',
      'urlThreadId',
      'clientThreadId',
      'serverThreadId',
      'threadId',
      'turn',
      'turnId',
      'turnIndex',
      'turns',
      'message',
      'messages',
      'displayItems',
      'conversationTurns',
      'isThreadContentLoading',
      'isNewThread',
      'when$',
      'fallback',
      'renderEmptyState',
      'renderEmptyFooter',
      'hideComposer',
      'isComposerPinnedToBottomOnEmptyState',
      'isScrolledFromBottom$',
      'shouldUseUnifiedComposer',
      'isCompletionInProgress',
      'isGizmoThread',
      'isProjectThread',
      'layoutMode',
      'currentModelId',
      'pageLoadSearchQuery',
      'forceRenderedTurnId',
      'items',
      'children',
    ]) {
      if (Object.prototype.hasOwnProperty.call(props, key)) {
        selected[key] = compactSelectedPropValue(key, props[key]);
      }
    }
    return {
      keys: objectKeys(props, 24),
      id: propHintString(props.id),
      role: propHintString(props.role),
      testid: propHintString(props['data-testid']),
      ariaHidden: propHintString(props['aria-hidden']),
      inert: !!props.inert,
      dataTurn: propHintString(props['data-turn']),
      dataMessageId: props['data-message-id'] ? idShape(String(props['data-message-id'])) : undefined,
      dataMessageAuthorRole: propHintString(props['data-message-author-role']),
      selectedValueShapes: selected,
    };
  }

  function conversationSubtreeSummary(rootFiber, limit = 80) {
    const nodes = [];
    const stack = [];
    const seen = new Set();
    try {
      if (rootFiber?.child) stack.push({fiber: rootFiber.child, depth: 1});
    } catch {}
    while (stack.length && nodes.length < limit && seen.size < 2500) {
      const {fiber, depth} = stack.pop();
      if (!fiber || seen.has(fiber)) continue;
      seen.add(fiber);
      const name = reactFiberName(fiber);
      let props = null;
      let hints = [];
      let stateNode = null;
      try {
        props = fiber.memoizedProps;
        hints = reactFiberHints(fiber, name, props);
        stateNode = fiber.stateNode?.nodeType === Node.ELEMENT_NODE
          ? elementBrief(fiber.stateNode)
          : null;
      } catch {}
      const stateNodeInteresting = !!stateNode && /thread|conversation|message|turn|composer|markdown|viewport|virtual|list/i.test([
        stateNode.id,
        stateNode.role,
        stateNode.testid,
        stateNode.dataTurn,
        ...(stateNode.classHints || []),
      ].join(' '));
      const propInteresting = hints.some((hint) => hint.startsWith('prop:'));
      if (
        propInteresting ||
        stateNodeInteresting ||
        /conversation|thread|turn|markdown|virtual|list|item/i.test(name)
      ) {
        const hookInteresting = propInteresting ||
          /^(NFe|BFe|e8|T2|qDr|\$Ar|XFe|pY|iyr)$/.test(name);
        nodes.push({
          depth,
          name,
          tag: Number(fiber.tag),
          hints,
          props: compactFiberPropsForTrace(props),
          sourceHint: shouldRecordFiberSourceHint(name, props) ? reactFiberSourceHint(fiber) : undefined,
          hooks: hookInteresting ? hookStateSummary(fiber, 8) : undefined,
          stateNode,
        });
      }
      try {
        if (fiber.sibling) stack.push({fiber: fiber.sibling, depth});
        if (fiber.child) stack.push({fiber: fiber.child, depth: depth + 1});
      } catch {}
    }
    return {
      visited: seen.size,
      count: nodes.length,
      nodes,
    };
  }

  function fiberForDomNode(node) {
    if (!node || typeof node !== 'object') return null;
    try {
      for (const key of Reflect.ownKeys(node)) {
        const label = String(key);
        if (label.startsWith('__reactFiber$') || label.startsWith('__reactInternalInstance$')) {
          return node[key] || null;
        }
      }
    } catch {}
    return null;
  }

  function reactFiberTagLabel(tag) {
    switch (Number(tag)) {
      case 0: return 'FunctionComponent';
      case 3: return 'HostRoot';
      case 5: return 'HostComponent';
      case 6: return 'HostText';
      case 7: return 'Fragment';
      case 10: return 'ContextProvider';
      case 11: return 'ForwardRef';
      case 13: return 'Suspense';
      case 14: return 'MemoComponent';
      case 15: return 'SimpleMemoComponent';
      case 22: return 'Offscreen';
      default: return `tag:${Number(tag)}`;
    }
  }

  function fiberNumericField(fiber, key) {
    try {
      const value = fiber?.[key];
      if (typeof value === 'number') return value;
      if (typeof value === 'bigint') return String(value);
    } catch {}
    return undefined;
  }

  function fiberInternalFieldSummary(fiber, key) {
    const summary = {};
    let value;
    try {
      value = fiber?.[key];
      summary.present = value !== null && value !== undefined;
      summary.shape = valueShape(value, 1);
    } catch (error) {
      summary.error = String(error?.name || error);
      return summary;
    }
    if (value && typeof value === 'object') {
      try {
        summary.ownValueShapes = ownValueShapes(value, 12);
      } catch (error) {
        summary.ownValueShapesError = String(error?.name || error);
      }
      try {
        summary.selectedValues = selectedNamedValueShapes(
          value,
          [/then/i, /wake/i, /lane/i, /cache/i, /retry/i, /transition/i, /pending/i, /base/i, /tree/i, /status/i, /value/i],
          12,
        );
      } catch (error) {
        summary.selectedValuesError = String(error?.name || error);
      }
    }
    return summary;
  }

  function suspenseChildChainSummary(startFiber, limit = 8) {
    const nodes = [];
    const seen = new Set();
    let current = startFiber || null;
    while (current && nodes.length < limit && !seen.has(current)) {
      seen.add(current);
      const name = reactFiberName(current);
      const tag = Number(current.tag);
      let props = null;
      let hints = [];
      let stateNode = null;
      try {
        props = current.memoizedProps;
        hints = reactFiberHints(current, name, props);
        stateNode = current.stateNode?.nodeType === Node.ELEMENT_NODE
          ? elementBrief(current.stateNode)
          : null;
      } catch {}
      nodes.push({
        index: nodes.length,
        name,
        tag,
        tagLabel: reactFiberTagLabel(tag),
        hints,
        lanes: fiberNumericField(current, 'lanes'),
        childLanes: fiberNumericField(current, 'childLanes'),
        flags: fiberNumericField(current, 'flags'),
        subtreeFlags: fiberNumericField(current, 'subtreeFlags'),
        memoizedState: tag === 13 || tag === 22
          ? fiberInternalFieldSummary(current, 'memoizedState')
          : undefined,
        props: compactFiberPropsForTrace(props),
        stateNode,
      });
      try {
        current = current.sibling || null;
      } catch {
        break;
      }
    }
    return nodes;
  }

  function fiberPathSummary(fiber, limit = 12) {
    const path = [];
    const seen = new Set();
    let current = fiber || null;
    while (current && path.length < limit && !seen.has(current)) {
      seen.add(current);
      const name = reactFiberName(current);
      const tag = Number(current.tag);
      path.push({
        name,
        tag,
        tagLabel: reactFiberTagLabel(tag),
      });
      try {
        current = current.return || null;
      } catch {
        break;
      }
    }
    return path.reverse();
  }

  function reactElementTypeSummary(type, props = null, depth = 0) {
    if (depth >= 3) return {kind: 'nested'};
    if (typeof type === 'string') return {kind: 'host', name: type};
    if (typeof type === 'function') {
      const name = String(type.displayName || type.name || '').slice(0, 80);
      return {
        kind: 'function',
        name,
        sourceHint: shouldRecordFiberSourceHint(name, props)
          ? Function.prototype.toString.call(type).replace(/\s+/g, ' ').slice(0, 2200)
          : undefined,
      };
    }
    if (!type || typeof type !== 'object') return valueShape(type, 1);
    const summary = {
      kind: 'object',
      tag: Object.prototype.toString.call(type),
      keys: objectKeys(type, 12),
      ownKeys: ownKeySummary(type, 12),
    };
    try {
      if (Object.prototype.hasOwnProperty.call(type, '_payload')) {
        const payload = type._payload;
        summary.lazyPayload = {
          shape: valueShape(payload, 1),
          ownValueShapes: ownValueShapes(payload, 8),
        };
        if (payload && typeof payload === 'object') {
          summary.lazyPayload.status = typeof payload._status === 'number'
            ? payload._status
            : undefined;
          summary.lazyPayload.result = valueShape(payload._result, 1);
          if (payload._result && typeof payload._result === 'object') {
            summary.lazyPayload.resultOwnValueShapes = ownValueShapes(payload._result, 8);
          }
        }
      }
      if (typeof type._init === 'function') {
        summary.hasLazyInit = true;
      }
      if (type.render) {
        summary.render = reactElementTypeSummary(type.render, props, depth + 1);
      }
      if (type.type) {
        summary.innerType = reactElementTypeSummary(type.type, props, depth + 1);
      }
    } catch (error) {
      summary.error = String(error?.name || error);
    }
    return summary;
  }

  function reactElementTreeSummary(value, depth = 0) {
    if (depth > 4) return {kind: 'nested'};
    if (Array.isArray(value)) {
      return {
        kind: 'array',
        length: value.length,
        items: value.slice(0, 6).map((item) => reactElementTreeSummary(item, depth + 1)),
      };
    }
    if (!value || typeof value !== 'object') return valueShape(value, 1);
    const maybeElement = Object.prototype.hasOwnProperty.call(value, '$$typeof') &&
      Object.prototype.hasOwnProperty.call(value, 'type') &&
      Object.prototype.hasOwnProperty.call(value, 'props');
    if (!maybeElement) return valueShape(value, 1);
    const props = value.props;
    const summary = {
      kind: 'react-element',
      key: value.key == null ? null : String(value.key).slice(0, 80),
      type: reactElementTypeSummary(value.type, props),
    };
    if (props && typeof props === 'object') {
      summary.propKeys = objectKeys(props, 16);
      const selected = {};
      for (const key of [
        'conversation',
        'isThreadContentLoading',
        'pageLoadSearchQuery',
        'conversationId',
        'clientThreadId',
        'serverThreadId',
        'threadId',
        'turnId',
      ]) {
        if (Object.prototype.hasOwnProperty.call(props, key)) {
          selected[key] = compactSelectedPropValue(key, props[key]);
        }
      }
      if (Object.keys(selected).length) summary.selectedProps = selected;
      if (Object.prototype.hasOwnProperty.call(props, 'children')) {
        summary.children = reactElementTreeSummary(props.children, depth + 1);
      }
      if (Object.prototype.hasOwnProperty.call(props, 'fallback')) {
        summary.fallback = reactElementTreeSummary(props.fallback, depth + 1);
      }
    }
    return summary;
  }

  function suspenseBoundaryInternalSummary(fiber) {
    let props = null;
    try {
      props = fiber?.memoizedProps || null;
    } catch {}
    return {
      mode: fiberNumericField(fiber, 'mode'),
      flags: fiberNumericField(fiber, 'flags'),
      subtreeFlags: fiberNumericField(fiber, 'subtreeFlags'),
      lanes: fiberNumericField(fiber, 'lanes'),
      childLanes: fiberNumericField(fiber, 'childLanes'),
      memoizedState: fiberInternalFieldSummary(fiber, 'memoizedState'),
      updateQueue: fiberInternalFieldSummary(fiber, 'updateQueue'),
      dependencies: fiberInternalFieldSummary(fiber, 'dependencies'),
      children: suspenseChildChainSummary(fiber?.child || null, 8),
      elementTree: reactElementTreeSummary(props?.children, 0),
      alternate: suspenseBoundaryAlternateSummary(fiber),
    };
  }

  function suspenseBoundaryAlternateSummary(fiber) {
    let alternate = null;
    try {
      alternate = fiber?.alternate || null;
    } catch {}
    if (!alternate) return {present: false};
    const tag = Number(alternate.tag);
    let props = null;
    try {
      props = alternate.memoizedProps || null;
    } catch {}
    return {
      present: true,
      name: reactFiberName(alternate),
      tag,
      tagLabel: reactFiberTagLabel(tag),
      mode: fiberNumericField(alternate, 'mode'),
      flags: fiberNumericField(alternate, 'flags'),
      subtreeFlags: fiberNumericField(alternate, 'subtreeFlags'),
      lanes: fiberNumericField(alternate, 'lanes'),
      childLanes: fiberNumericField(alternate, 'childLanes'),
      memoizedState: tag === 13 || tag === 22
        ? fiberInternalFieldSummary(alternate, 'memoizedState')
        : undefined,
      updateQueue: fiberInternalFieldSummary(alternate, 'updateQueue'),
      dependencies: fiberInternalFieldSummary(alternate, 'dependencies'),
      children: suspenseChildChainSummary(alternate.child || null, 8),
      elementTree: reactElementTreeSummary(props?.children, 0),
    };
  }

  function reactRootLaneSummary() {
    let rootFiber = null;
    let root = null;
    try {
      rootFiber = lastReactRoot?.current || lastReactRoot || null;
      root = rootFiber?.stateNode || null;
    } catch {}
    const summary = {
      rootPresent: !!rootFiber,
      stateNodePresent: !!root,
      rootFiberLanes: fiberNumericField(rootFiber, 'lanes'),
      rootFiberChildLanes: fiberNumericField(rootFiber, 'childLanes'),
    };
    if (!root || typeof root !== 'object') return summary;
    for (const key of [
      'pendingLanes',
      'suspendedLanes',
      'pingedLanes',
      'expiredLanes',
      'errorRecoveryDisabledLanes',
      'shellSuspendCounter',
      'entangledLanes',
      'finishedLanes',
    ]) {
      const value = fiberNumericField(root, key);
      if (value !== undefined) summary[key] = value;
    }
    try {
      if (root.entanglements) summary.entanglementsShape = valueShape(root.entanglements, 1);
      if (root.hiddenUpdates) summary.hiddenUpdatesShape = valueShape(root.hiddenUpdates, 1);
    } catch {}
    return summary;
  }

  function suspenseBoundaryProbes(limit = 24) {
    const boundaries = [];
    const stack = [];
    const seen = new Set();
    let threadFiber = null;
    try {
      const rootFiber = lastReactRoot?.current || lastReactRoot || null;
      if (rootFiber) stack.push({fiber: rootFiber, depth: 0, source: 'root'});
    } catch {}
    try {
      threadFiber = fiberForDomNode(document.getElementById('thread'));
      if (threadFiber) stack.push({fiber: threadFiber, depth: 0, source: 'thread'});
    } catch {}
    while (stack.length && boundaries.length < limit && seen.size < 8000) {
      const {fiber, depth, source} = stack.pop();
      if (!fiber || seen.has(fiber)) continue;
      seen.add(fiber);
      const tag = Number(fiber.tag);
      if (tag === 13 || tag === 22) {
        const name = reactFiberName(fiber);
        let props = null;
        let hints = [];
        try {
          props = fiber.memoizedProps;
          hints = reactFiberHints(fiber, name, props);
        } catch {}
        boundaries.push({
          source,
          depth,
          name,
          tag,
          tagLabel: reactFiberTagLabel(tag),
          hints,
          path: fiberPathSummary(fiber, 12),
          props: compactFiberPropsForTrace(props),
          internal: suspenseBoundaryInternalSummary(fiber),
        });
      }
      try {
        if (fiber.sibling) stack.push({fiber: fiber.sibling, depth, source});
        if (fiber.child) stack.push({fiber: fiber.child, depth: depth + 1, source});
      } catch {}
    }
    return {
      rootPresent: !!lastReactRoot,
      threadFiberPresent: !!threadFiber,
      rootLanes: reactRootLaneSummary(),
      visited: seen.size,
      count: boundaries.length,
      boundaries,
    };
  }

  function compactFiberForTrace(fiber, depth = 0) {
    if (!fiber) return null;
    let props = null;
    let stateNode = null;
    let name = '';
    let hints = [];
    try {
      name = reactFiberName(fiber);
      props = fiber.memoizedProps;
      hints = reactFiberHints(fiber, name, props);
      stateNode = fiber.stateNode?.nodeType === Node.ELEMENT_NODE
        ? elementBrief(fiber.stateNode)
        : null;
    } catch {}
    return {
      depth,
      name,
      tag: Number(fiber.tag),
      hints,
      props: compactFiberPropsForTrace(props),
      sourceHint: shouldRecordFiberSourceHint(name, props) ? reactFiberSourceHint(fiber) : undefined,
      hooks: (hints.some((hint) => hint.startsWith('prop:')) ||
        /^(NFe|BFe|e8|T2|qDr|\$Ar|XFe|pY|iyr)$/.test(name))
        ? hookStateSummary(fiber, 8)
        : undefined,
      stateNode,
    };
  }

  function fiberAncestorSummary(fiber, limit = 40) {
    const nodes = [];
    const seen = new Set();
    let current = fiber;
    while (current && nodes.length < limit && !seen.has(current)) {
      seen.add(current);
      nodes.push(compactFiberForTrace(current, nodes.length));
      try {
        current = current.return || null;
      } catch {
        break;
      }
    }
    return nodes;
  }

  function fiberDescendantSummary(rootFiber, limit = 120) {
    const nodes = [];
    const stack = [];
    const seen = new Set();
    try {
      if (rootFiber?.child) stack.push({fiber: rootFiber.child, depth: 1});
    } catch {}
    while (stack.length && nodes.length < limit && seen.size < 2500) {
      const {fiber, depth} = stack.pop();
      if (!fiber || seen.has(fiber)) continue;
      seen.add(fiber);
      const compact = compactFiberForTrace(fiber, depth);
      const selected = compact?.props?.selectedValueShapes || {};
      const hasSelectedProps = Object.keys(selected).length > 0;
      const stateNode = compact?.stateNode;
      const stateNodeInteresting = !!stateNode && /thread|conversation|message|turn|composer|markdown|viewport|virtual|list/i.test([
        stateNode.id,
        stateNode.role,
        stateNode.testid,
        stateNode.dataTurn,
        ...(stateNode.classHints || []),
      ].join(' '));
      const name = compact?.name || '';
      if (
        hasSelectedProps ||
        stateNodeInteresting ||
        /conversation|thread|turn|markdown|virtual|list|item/i.test(name)
      ) {
        nodes.push(compact);
      }
      try {
        if (fiber.sibling) stack.push({fiber: fiber.sibling, depth});
        if (fiber.child) stack.push({fiber: fiber.child, depth: depth + 1});
      } catch {}
    }
    return {
      visited: seen.size,
      count: nodes.length,
      nodes,
    };
  }

  function reactThreadFiberSnapshot() {
    const thread = document.getElementById('thread');
    const fiber = fiberForDomNode(thread);
    return {
      thread: elementBrief(thread),
      fiberPresent: !!fiber,
      ancestors: fiberAncestorSummary(fiber, 36),
      subtree: fiberDescendantSummary(fiber, 160),
    };
  }

  function installReactCommitTrace() {
    const existing = window.__REACT_DEVTOOLS_GLOBAL_HOOK__;
    if (existing?.__lmChatGPTLiveTraceWrapped) {
      reactCommitStats.hookInstalled = true;
      return;
    }
    const renderers = existing?.renderers instanceof Map ? existing.renderers : new Map();
    let nextRendererId = renderers.size || 0;
    const nativeInject = typeof existing?.inject === 'function' ? existing.inject.bind(existing) : null;
    const nativeCommitRoot = typeof existing?.onCommitFiberRoot === 'function'
      ? existing.onCommitFiberRoot.bind(existing)
      : null;
    const nativeCommitUnmount = typeof existing?.onCommitFiberUnmount === 'function'
      ? existing.onCommitFiberUnmount.bind(existing)
      : null;
    const hook = {
      ...(existing || {}),
      supportsFiber: true,
      renderers,
      __lmChatGPTLiveTraceWrapped: true,
      inject(renderer) {
        let id;
        try {
          id = nativeInject ? nativeInject(renderer) : undefined;
        } catch (error) {
          record('react-devtools-inject-error', {name: error?.name || '', message: String(error?.message || error).slice(0, 200)});
        }
        if (typeof id !== 'number') id = ++nextRendererId;
        try { renderers.set(id, renderer); } catch {}
        reactCommitStats.rendererCount = renderers.size;
        reactCommitStats.lastRendererId = id;
        record('react-renderer-inject', {
          id,
          version: String(renderer?.version || '').slice(0, 80),
          packageName: String(renderer?.rendererPackageName || '').slice(0, 80),
        });
        return id;
      },
      onCommitFiberRoot(id, root, priorityLevel, didError) {
        reactCommitStats.commitCount += 1;
        reactCommitStats.lastCommitAt = nowMs();
        reactCommitStats.lastRendererId = Number(id) || reactCommitStats.lastRendererId;
        lastReactRoot = root || lastReactRoot;
        recordConversationIdentitySnapshot(root, 'commit');
        recordConversationMaterializationSnapshot('commit');
        try {
          const summary = summarizeReactCommitRoot(root);
          reactCommitStats.lastCommit = {
            id,
            didError: !!didError,
            priorityLevel: typeof priorityLevel === 'number' ? priorityLevel : undefined,
            ...summary,
          };
          if (
            summary.conversationHints ||
            summary.messageHints ||
            summary.turnHints ||
            summary.markdownHints ||
            summary.dataTurnProps ||
            summary.dataMessageProps ||
            summary.dataRoleProps
          ) {
            const sample = {time: nowMs(), id, ...summary};
            pushReactFiberSample(sample);
            record('react-commit', sample);
          }
        } catch (error) {
          reactCommitStats.commitErrors += 1;
          record('react-commit-trace-error', {
            name: error?.name || '',
            message: String(error?.message || error).slice(0, 240),
          });
        }
        if (nativeCommitRoot) {
          try {
            return nativeCommitRoot(id, root, priorityLevel, didError);
          } catch (error) {
            record('react-devtools-commit-error', {name: error?.name || '', message: String(error?.message || error).slice(0, 200)});
          }
        }
      },
      onCommitFiberUnmount(id, fiber) {
        if (nativeCommitUnmount) {
          try {
            return nativeCommitUnmount(id, fiber);
          } catch {}
        }
      },
      sub(event, fn) {
        try {
          return typeof existing?.sub === 'function' ? existing.sub(event, fn) : () => {};
        } catch {
          return () => {};
        }
      },
      on(event, fn) {
        try {
          return typeof existing?.on === 'function' ? existing.on(event, fn) : undefined;
        } catch {}
      },
      off(event, fn) {
        try {
          return typeof existing?.off === 'function' ? existing.off(event, fn) : undefined;
        } catch {}
      },
      emit(event, data) {
        try {
          return typeof existing?.emit === 'function' ? existing.emit(event, data) : undefined;
        } catch {}
      },
    };
    reactCommitStats.hookInstalled = true;
    reactCommitStats.hookPreexisting = !!existing;
    window.__REACT_DEVTOOLS_GLOBAL_HOOK__ = hook;
  }

  function all(selectors) {
    for (const selector of selectors) {
      try {
        const found = [...document.querySelectorAll(selector)];
        if (found.length) return found;
      } catch {}
    }
    return [];
  }

  function conversationDomState() {
    const assistantTexts = all([
      '[data-message-author-role="assistant"]',
      '[data-testid*="assistant" i]',
      '[class*="assistant" i] .markdown',
      '.markdown',
    ]).map(textOf).filter(Boolean);
    const userTexts = all([
      '[data-message-author-role="user"]',
      '[data-testid*="user" i]',
    ]).map(textOf).filter(Boolean);
    const stopButtons = all([
      'button[data-testid*="stop" i]',
      'button[aria-label*="Stop" i]',
      'button[aria-label*="Cancel" i]',
    ]);
    return {
      url: compactUrl(location.href),
      readyState: document.readyState,
      assistantCount: assistantTexts.length,
      latestAssistantLen: assistantTexts.length ? assistantTexts[assistantTexts.length - 1].length : 0,
      userCount: userTexts.length,
      latestUserLen: userTexts.length ? userTexts[userTexts.length - 1].length : 0,
      stopButtonCount: stopButtons.length,
      bodyTextLen: textOf(document.body).length,
      selectorCensus: selectorCensus(),
      domTree: domTreeProbe(),
      appRuntime: appRuntimeState(),
      eventLoop: eventLoopState(),
      messageTasks: messageTaskState(),
      observerApis: observerApiState(),
      domMutations: domMutationState(),
      reactCommits: reactCommitState(),
    };
  }

  function recordDomState(reason, extra = {}) {
    const state = conversationDomState();
    const signature = JSON.stringify(state);
    if (signature === lastDomSignature && reason === 'mutation') return;
    lastDomSignature = signature;
    record('dom-state', {reason, ...extra, state});
  }

  function installMutationTrace() {
    const target = document.documentElement || document.body;
    if (!target || target.__lmChatGPTLiveTraceMutationObserver) return;
    const observer = new MutationObserver((mutations) => {
      let addedNodes = 0;
      let removedNodes = 0;
      let textMutations = 0;
      for (const mutation of mutations) {
        addedNodes += mutation.addedNodes ? mutation.addedNodes.length : 0;
        removedNodes += mutation.removedNodes ? mutation.removedNodes.length : 0;
        if (mutation.type === 'characterData') textMutations += 1;
      }
      recordDomState('mutation', {mutations: mutations.length, addedNodes, removedNodes, textMutations});
    });
    observer.observe(target, {subtree: true, childList: true, characterData: true});
    target.__lmChatGPTLiveTraceMutationObserver = observer;
    recordDomState('observer-start');
  }

  function installEventLoopTrace() {
    const nativeSetTimeout = window.setTimeout;
    const nativeSetInterval = window.setInterval;
    const nativeRequestAnimationFrame = window.requestAnimationFrame;
    const nativeRequestIdleCallback = window.requestIdleCallback;
    const nativeSchedulerPostTask = window.scheduler?.postTask;
    const nativeQueueMicrotask = window.queueMicrotask;
    if (typeof nativeSetTimeout === 'function') {
      window.setTimeout = function tracedSetTimeout(callback, delay, ...args) {
        eventLoopStats.timeoutScheduled += 1;
        if (typeof callback !== 'function') return nativeSetTimeout.call(this, callback, delay, ...args);
        return nativeSetTimeout.call(this, function tracedTimeoutCallback(...callbackArgs) {
          eventLoopStats.timeoutFired += 1;
          eventLoopStats.lastTimeoutFiredAt = nowMs();
          return callback.apply(this, callbackArgs);
        }, delay, ...args);
      };
    }
    if (typeof nativeSetInterval === 'function') {
      window.setInterval = function tracedSetInterval(callback, delay, ...args) {
        eventLoopStats.intervalScheduled += 1;
        if (typeof callback !== 'function') return nativeSetInterval.call(this, callback, delay, ...args);
        return nativeSetInterval.call(this, function tracedIntervalCallback(...callbackArgs) {
          eventLoopStats.intervalFired += 1;
          eventLoopStats.lastIntervalFiredAt = nowMs();
          return callback.apply(this, callbackArgs);
        }, delay, ...args);
      };
      nativeSetInterval.call(window, () => {
        eventLoopStats.heartbeat += 1;
        eventLoopStats.lastHeartbeatAt = nowMs();
      }, 1000);
    }
    if (typeof nativeRequestAnimationFrame === 'function') {
      window.requestAnimationFrame = function tracedRequestAnimationFrame(callback) {
        eventLoopStats.rafScheduled += 1;
        if (typeof callback !== 'function') return nativeRequestAnimationFrame.call(this, callback);
        return nativeRequestAnimationFrame.call(this, function tracedAnimationFrameCallback(timestamp) {
          eventLoopStats.rafFired += 1;
          eventLoopStats.lastRafFiredAt = nowMs();
          return callback.call(this, timestamp);
        });
      };
    }
    if (typeof nativeRequestIdleCallback === 'function') {
      window.requestIdleCallback = function tracedRequestIdleCallback(callback, options) {
        eventLoopStats.idleScheduled += 1;
        if (typeof callback !== 'function') return nativeRequestIdleCallback.call(this, callback, options);
        return nativeRequestIdleCallback.call(this, function tracedIdleCallback(deadline) {
          eventLoopStats.idleFired += 1;
          eventLoopStats.lastIdleFiredAt = nowMs();
          return callback.call(this, deadline);
        }, options);
      };
    }
    if (typeof nativeSchedulerPostTask === 'function') {
      window.scheduler.postTask = function tracedSchedulerPostTask(callback, options) {
        eventLoopStats.schedulerPostTaskScheduled += 1;
        const result = nativeSchedulerPostTask.call(this, callback, options);
        if (result?.then) {
          result.then(
            () => {
              eventLoopStats.schedulerPostTaskSettled += 1;
              eventLoopStats.lastSchedulerPostTaskSettledAt = nowMs();
            },
            () => {
              eventLoopStats.schedulerPostTaskSettled += 1;
              eventLoopStats.lastSchedulerPostTaskSettledAt = nowMs();
            },
          );
        }
        return result;
      };
    }
    if (typeof nativeQueueMicrotask === 'function') {
      window.queueMicrotask = function tracedQueueMicrotask(callback) {
        eventLoopStats.microtaskScheduled += 1;
        if (typeof callback !== 'function') return nativeQueueMicrotask.call(this, callback);
        return nativeQueueMicrotask.call(this, function tracedMicrotaskCallback() {
          eventLoopStats.microtaskFired += 1;
          eventLoopStats.lastMicrotaskFiredAt = nowMs();
          return callback.call(this);
        });
      };
    }
  }

  function installHistoryTrace() {
    for (const name of ['pushState', 'replaceState']) {
      const nativeMethod = history[name];
      if (typeof nativeMethod !== 'function') continue;
      history[name] = function tracedHistoryMethod(state, title, url) {
        const result = nativeMethod.apply(this, arguments);
        record('history', {method: name, url: compactUrl(url || location.href), stateKind: typeof state});
        recordDomState(`history-${name}`);
        return result;
      };
    }
    window.addEventListener('popstate', () => {
      record('history', {method: 'popstate', url: compactUrl(location.href)});
      recordDomState('history-popstate');
    });
    window.addEventListener('hashchange', () => {
      record('history', {method: 'hashchange', url: compactUrl(location.href)});
      recordDomState('history-hashchange');
    });
  }

  function navigationEntrySummary(entry) {
    if (!entry || typeof entry !== 'object') return null;
    const summary = {};
    try {
      summary.url = compactUrl(entry.url || '');
    } catch {}
    for (const key of ['key', 'id', 'index', 'sameDocument']) {
      try {
        const value = entry[key];
        if (typeof value === 'string') summary[key] = idShape(value);
        else if (typeof value === 'number' || typeof value === 'boolean') summary[key] = value;
      } catch {}
    }
    try {
      const state = typeof entry.getState === 'function' ? entry.getState() : entry.state;
      summary.stateShape = valueShape(state, 1);
    } catch (error) {
      summary.stateError = String(error?.name || error);
    }
    return summary;
  }

  function navigationTraceState() {
    const nav = window.navigation;
    const currentEntry = nav && typeof nav === 'object' ? nav.currentEntry : null;
    return {
      ...navigationTraceStats,
      present: !!nav,
      canGoBack: !!nav?.canGoBack,
      canGoForward: !!nav?.canGoForward,
      currentEntry: navigationEntrySummary(currentEntry),
      sampleCount: navigationTraceSamples.length,
      samples: navigationTraceSamples.slice(-40),
    };
  }

  function installNavigateEventInterceptTrace(event) {
    if (!event || event.__lmChatGPTLiveTraceInterceptWrapped) return;
    const nativeIntercept = event.intercept;
    if (typeof nativeIntercept !== 'function') return;
    try {
      event.intercept = function tracedNavigateEventIntercept(options = {}) {
        navigationTraceStats.interceptCalls += 1;
        navigationTraceStats.lastInterceptAt = nowMs();
        const sample = {
          op: 'intercept-call',
          navigationType: String(event.navigationType || ''),
          optionsKeys: options && typeof options === 'object' ? objectKeys(options, 12) : [],
          currentEntry: navigationEntrySummary(window.navigation?.currentEntry),
          destination: navigationEntrySummary(event.destination),
        };
        pushNavigationTraceSample(sample);
        record('navigation-api', sample);
        let nextOptions = options;
        try {
          if (options && typeof options === 'object' && typeof options.handler === 'function') {
            const nativeHandler = options.handler;
            nextOptions = {
              ...options,
              handler(...handlerArgs) {
                navigationTraceStats.interceptHandlerStarted += 1;
                navigationTraceStats.lastInterceptHandlerAt = nowMs();
                record('navigation-api', {
                  op: 'intercept-handler-start',
                  navigationType: String(event.navigationType || ''),
                });
                let result;
                try {
                  result = nativeHandler.apply(this, handlerArgs);
                } catch (error) {
                  navigationTraceStats.interceptHandlerRejected += 1;
                  record('navigation-api', {
                    op: 'intercept-handler-throw',
                    name: String(error?.name || ''),
                    message: String(error?.message || error).slice(0, 240),
                  });
                  throw error;
                }
                if (result?.then) {
                  result.then(
                    () => {
                      navigationTraceStats.interceptHandlerSettled += 1;
                      navigationTraceStats.lastInterceptHandlerAt = nowMs();
                      record('navigation-api', {op: 'intercept-handler-resolved'});
                      recordAppState('navigation-intercept-handler-resolved');
                    },
                    (error) => {
                      navigationTraceStats.interceptHandlerRejected += 1;
                      navigationTraceStats.lastInterceptHandlerAt = nowMs();
                      record('navigation-api', {
                        op: 'intercept-handler-rejected',
                        name: String(error?.name || ''),
                        message: String(error?.message || error).slice(0, 240),
                      });
                      recordAppState('navigation-intercept-handler-rejected');
                    },
                  );
                } else {
                  navigationTraceStats.interceptHandlerSettled += 1;
                  navigationTraceStats.lastInterceptHandlerAt = nowMs();
                  record('navigation-api', {op: 'intercept-handler-return'});
                  recordAppState('navigation-intercept-handler-return');
                }
                return result;
              },
            };
          }
        } catch {}
        return nativeIntercept.call(this, nextOptions);
      };
      Object.defineProperty(event, '__lmChatGPTLiveTraceInterceptWrapped', {
        value: true,
        configurable: true,
      });
    } catch (error) {
      pushNavigationTraceSample({
        op: 'intercept-wrap-error',
        name: String(error?.name || ''),
        message: String(error?.message || error).slice(0, 240),
      });
    }
  }

  function installNavigationApiTrace() {
    const nav = window.navigation;
    if (!nav || typeof nav !== 'object') return;
    navigationTraceStats.present = true;
    if (!nav.__lmChatGPTLiveTraceNavigationWrapped) {
      const nativeNavigate = nav.navigate;
      if (typeof nativeNavigate === 'function') {
        try {
          nav.navigate = function tracedNavigationNavigate(url, options) {
            navigationTraceStats.navigateCalls += 1;
            navigationTraceStats.lastNavigateAt = nowMs();
            const sample = {
              op: 'navigate-call',
              url: compactUrl(url || ''),
              optionsKeys: options && typeof options === 'object' ? objectKeys(options, 12) : [],
              currentEntry: navigationEntrySummary(nav.currentEntry),
            };
            pushNavigationTraceSample(sample);
            record('navigation-api', sample);
            let result;
            try {
              result = nativeNavigate.apply(this, arguments);
            } catch (error) {
              navigationTraceStats.navigateErrors += 1;
              const errorSample = {
                op: 'navigate-throw',
                name: String(error?.name || ''),
                message: String(error?.message || error).slice(0, 240),
              };
              pushNavigationTraceSample(errorSample);
              record('navigation-api', errorSample);
              throw error;
            }
            if (result?.committed?.then) {
              result.committed.then(
                (entry) => {
                  navigationTraceStats.navigateCommitted += 1;
                  const commitSample = {
                    op: 'navigate-committed',
                    entry: navigationEntrySummary(entry),
                    currentEntry: navigationEntrySummary(nav.currentEntry),
                  };
                  pushNavigationTraceSample(commitSample);
                  record('navigation-api', commitSample);
                  recordAppState('navigation-committed');
                },
                (error) => {
                  navigationTraceStats.navigateCommitRejected += 1;
                  const rejectSample = {
                    op: 'navigate-commit-rejected',
                    name: String(error?.name || ''),
                    message: String(error?.message || error).slice(0, 240),
                  };
                  pushNavigationTraceSample(rejectSample);
                  record('navigation-api', rejectSample);
                },
              );
            }
            if (result?.finished?.then) {
              result.finished.then(
                (entry) => {
                  navigationTraceStats.navigateFinished += 1;
                  const finishSample = {
                    op: 'navigate-finished',
                    entry: navigationEntrySummary(entry),
                    currentEntry: navigationEntrySummary(nav.currentEntry),
                  };
                  pushNavigationTraceSample(finishSample);
                  record('navigation-api', finishSample);
                  recordAppState('navigation-finished');
                },
                (error) => {
                  navigationTraceStats.navigateFinishRejected += 1;
                  const rejectSample = {
                    op: 'navigate-finish-rejected',
                    name: String(error?.name || ''),
                    message: String(error?.message || error).slice(0, 240),
                  };
                  pushNavigationTraceSample(rejectSample);
                  record('navigation-api', rejectSample);
                },
              );
            }
            return result;
          };
        } catch (error) {
          pushNavigationTraceSample({
            op: 'navigate-wrap-error',
            name: String(error?.name || ''),
            message: String(error?.message || error).slice(0, 240),
          });
        }
      }
      try {
        Object.defineProperty(nav, '__lmChatGPTLiveTraceNavigationWrapped', {
          value: true,
          configurable: true,
        });
      } catch {}
    }
    for (const type of ['navigate', 'currententrychange', 'navigatesuccess', 'navigateerror']) {
      try {
        nav.addEventListener(type, (event) => {
          navigationTraceStats.lastEventAt = nowMs();
          if (type === 'navigate') navigationTraceStats.navigateEvents += 1;
          if (type === 'currententrychange') navigationTraceStats.currentEntryChanges += 1;
          if (type === 'navigatesuccess') navigationTraceStats.navigateSuccess += 1;
          if (type === 'navigateerror') navigationTraceStats.navigateError += 1;
          if (type === 'navigate') installNavigateEventInterceptTrace(event);
          const sample = {
            op: type,
            navigationType: String(event.navigationType || ''),
            canIntercept: !!event.canIntercept,
            hashChange: !!event.hashChange,
            userInitiated: !!event.userInitiated,
            destination: navigationEntrySummary(event.destination),
            from: navigationEntrySummary(event.from),
            currentEntry: navigationEntrySummary(nav.currentEntry),
          };
          pushNavigationTraceSample(sample);
          record('navigation-api', sample);
          recordAppState(`navigation-${type}`);
        });
      } catch {}
    }
  }

  function looksLikeThreadId(value) {
    if (typeof value !== 'string') return false;
    if (/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(value)) {
      return true;
    }
    return /^[0-9a-f]{40}$/i.test(value);
  }

  function idMapTraceState() {
    return {
      ...idMapTraceStats,
      sampleCount: idMapTraceSamples.length,
      samples: idMapTraceSamples.slice(-40),
    };
  }

  function installIdMapTrace() {
    const proto = Map.prototype;
    if (proto.__lmChatGPTLiveTraceIdMapWrapped) return;
    const nativeSet = proto.set;
    const nativeGet = proto.get;
    if (typeof nativeSet === 'function') {
      proto.set = function tracedMapSet(key, value) {
        if (looksLikeThreadId(key)) {
          idMapTraceStats.set += 1;
          idMapTraceStats.setThreadKey += 1;
          idMapTraceStats.lastSetAt = nowMs();
          const sample = {
            op: 'set',
            key: idShape(key),
            value: looksLikeThreadId(value) ? idShape(value) : undefined,
            valueShape: valueShape(value, 1),
            valueFields: semanticScalarFields(value),
            keyLength: key.length,
            valueLength: typeof value === 'string' ? value.length : undefined,
            mapSizeBefore: typeof this?.size === 'number' ? this.size : undefined,
          };
          pushIdMapTraceSample(sample);
          record('id-map-set', sample);
        } else if (looksLikeThreadId(value)) {
          idMapTraceStats.set += 1;
          idMapTraceStats.lastSetAt = nowMs();
          const sample = {
            op: 'set-value-id',
            keyShape: valueShape(key, 1),
            value: idShape(value),
            valueLength: value.length,
            mapSizeBefore: typeof this?.size === 'number' ? this.size : undefined,
          };
          pushIdMapTraceSample(sample);
          record('id-map-set', sample);
        }
        return nativeSet.apply(this, arguments);
      };
    }
    if (typeof nativeGet === 'function') {
      proto.get = function tracedMapGet(key) {
        const result = nativeGet.apply(this, arguments);
        if (looksLikeThreadId(key)) {
          idMapTraceStats.get += 1;
          idMapTraceStats.lastGetAt = nowMs();
          const stringHit = looksLikeThreadId(result);
          const present = result !== undefined;
          if (stringHit) idMapTraceStats.getHit += 1;
          else idMapTraceStats.getMiss += 1;
          if (present) idMapTraceStats.getPresent += 1;
          else idMapTraceStats.getUndefined += 1;
          if (present || idMapTraceSamples.length < 20) {
            const sample = {
              op: 'get',
              key: idShape(key),
              result: stringHit ? idShape(result) : undefined,
              stringHit,
              present,
              resultShape: valueShape(result, 1),
              resultFields: semanticScalarFields(result),
              keyLength: key.length,
              resultLength: typeof result === 'string' ? result.length : undefined,
              mapSize: typeof this?.size === 'number' ? this.size : undefined,
            };
            pushIdMapTraceSample(sample);
            record('id-map-get', sample);
          }
        }
        return result;
      };
    }
    Object.defineProperty(proto, '__lmChatGPTLiveTraceIdMapWrapped', {
      value: true,
      configurable: true,
    });
  }

  function summarizeData(data) {
    function summarizeJsonValue(value, depth = 0) {
      if (depth > 2) return {kind: 'nested'};
      if (Array.isArray(value)) {
        return {
          kind: 'array',
          length: value.length,
          items: value.slice(0, 4).map((item) => summarizeJsonValue(item, depth + 1)),
        };
      }
      if (value && typeof value === 'object') {
        const keys = Object.keys(value).slice(0, 12);
        const safeFields = {};
        for (const key of ['type', 'event', 'kind', 'action', 'status', 'state', 'phase', 'message_type']) {
          const field = value[key];
          if (typeof field === 'string' || typeof field === 'number' || typeof field === 'boolean') {
            safeFields[key] = field;
          }
        }
        return {kind: 'object', keys, fields: safeFields};
      }
      if (typeof value === 'string') return {kind: 'string', length: value.length};
      return {kind: typeof value};
    }

    if (typeof data === 'string') {
      const summary = {kind: 'string', length: data.length};
      try {
        const parsed = JSON.parse(data);
        if (parsed && typeof parsed === 'object') {
          summary.json = summarizeJsonValue(parsed);
          summary.jsonKeys = Object.keys(parsed).slice(0, 8);
          if (typeof parsed.type === 'string') summary.jsonType = parsed.type;
          if (typeof parsed.event === 'string') summary.jsonEvent = parsed.event;
        }
      } catch {}
      return summary;
    }
    if (data instanceof ArrayBuffer) return {kind: 'arraybuffer', byteLength: data.byteLength};
    if (ArrayBuffer.isView(data)) return {kind: data.constructor?.name || 'typedarray', byteLength: data.byteLength};
    if (typeof Blob !== 'undefined' && data instanceof Blob) {
      return {kind: 'blob', size: data.size, type: data.type || ''};
    }
    return {kind: typeof data};
  }

  function installMessageTaskTrace() {
    const NativeMessageChannel = window.MessageChannel;
    if (typeof NativeMessageChannel === 'function') {
      function wrapPort(port, portName) {
        if (!port || port.__lmChatGPTLiveTraceWrapped) return port;
        try {
          Object.defineProperty(port, '__lmChatGPTLiveTraceWrapped', {value: true});
        } catch {}
        try {
          const descriptor = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(port), 'onmessage');
          if (descriptor?.set && descriptor?.get) {
            Object.defineProperty(port, 'onmessage', {
              configurable: true,
              enumerable: descriptor.enumerable,
              get() {
                return descriptor.get.call(this);
              },
              set(callback) {
                messageTaskStats.messagePortOnmessageSet += 1;
                messageTaskStats.lastMessagePortOnmessageSetAt = nowMs();
                record('message-port-onmessage-set', {port: portName, callbackKind: typeof callback});
                if (typeof callback !== 'function') return descriptor.set.call(this, callback);
                const wrapped = function tracedPortOnmessage(event) {
                  messageTaskStats.messagePortOnmessageFired += 1;
                  messageTaskStats.lastMessagePortOnmessageFiredAt = nowMs();
                  record('message-port-onmessage-fired', {port: portName, data: summarizeData(event?.data)});
                  recordAppState('message-port-onmessage-fired');
                  return callback.apply(this, arguments);
                };
                return descriptor.set.call(this, wrapped);
              },
            });
          }
        } catch {}
        try {
          const nativeAddEventListener = port.addEventListener;
          const nativeRemoveEventListener = port.removeEventListener;
          const listenerWrappers = new WeakMap();
          if (typeof nativeAddEventListener === 'function') {
            port.addEventListener = function tracedPortAddEventListener(type, listener) {
              if (String(type) !== 'message' || typeof listener !== 'function') {
                return nativeAddEventListener.apply(this, arguments);
              }
              messageTaskStats.messagePortListenerAdded += 1;
              messageTaskStats.lastMessagePortListenerAddedAt = nowMs();
              record('message-port-listener-add', {port: portName});
              let wrapped = listenerWrappers.get(listener);
              if (!wrapped) {
                wrapped = function tracedMessagePortListener(event) {
                  messageTaskStats.messagePortListenerFired += 1;
                  messageTaskStats.lastMessagePortListenerFiredAt = nowMs();
                  record('message-port-listener-fired', {port: portName, data: summarizeData(event?.data)});
                  recordAppState('message-port-listener-fired');
                  return listener.apply(this, arguments);
                };
                listenerWrappers.set(listener, wrapped);
              }
              return nativeAddEventListener.call(this, type, wrapped, arguments[2]);
            };
          }
          if (typeof nativeRemoveEventListener === 'function') {
            port.removeEventListener = function tracedPortRemoveEventListener(type, listener) {
              const wrapped = typeof listener === 'function' ? listenerWrappers.get(listener) : undefined;
              return nativeRemoveEventListener.call(this, type, wrapped || listener, arguments[2]);
            };
          }
        } catch {}
        try {
          const nativePostMessage = port.postMessage;
          if (typeof nativePostMessage === 'function') {
            port.postMessage = function tracedPortPostMessage(message) {
              messageTaskStats.messagePortPostMessage += 1;
              messageTaskStats.lastMessagePortPostMessageAt = nowMs();
              record('message-port-post', {port: portName, data: summarizeData(message)});
              return nativePostMessage.apply(this, arguments);
            };
          }
        } catch {}
        try {
          const nativeStart = port.start;
          if (typeof nativeStart === 'function') {
            port.start = function tracedPortStart() {
              messageTaskStats.messagePortStart += 1;
              return nativeStart.apply(this, arguments);
            };
          }
        } catch {}
        try {
          port.addEventListener?.('message', (event) => {
            messageTaskStats.messagePortMessage += 1;
            messageTaskStats.lastMessagePortMessageAt = nowMs();
            record('message-port-message', {port: portName, data: summarizeData(event.data)});
            recordAppState('message-port-message');
          });
        } catch {}
        return port;
      }

      function TracedMessageChannel() {
        messageTaskStats.messageChannelConstructed += 1;
        messageTaskStats.lastMessageChannelConstructedAt = nowMs();
        record('message-channel-create');
        const channel = new NativeMessageChannel();
        wrapPort(channel.port1, 'port1');
        wrapPort(channel.port2, 'port2');
        return channel;
      }
      TracedMessageChannel.prototype = NativeMessageChannel.prototype;
      Object.setPrototypeOf(TracedMessageChannel, NativeMessageChannel);
      window.MessageChannel = TracedMessageChannel;
    }

    const nativeWindowPostMessage = window.postMessage;
    if (typeof nativeWindowPostMessage === 'function') {
      window.postMessage = function tracedWindowPostMessage(message, targetOrigin) {
        messageTaskStats.windowPostMessage += 1;
        messageTaskStats.lastWindowPostMessageAt = nowMs();
        record('window-post-message', {
          targetOrigin: String(targetOrigin || '').slice(0, 120),
          data: summarizeData(message),
        });
        return nativeWindowPostMessage.apply(this, arguments);
      };
      window.addEventListener('message', (event) => {
        messageTaskStats.windowMessage += 1;
        messageTaskStats.lastWindowMessageAt = nowMs();
        record('window-message', {
          origin: String(event.origin || '').slice(0, 120),
          data: summarizeData(event.data),
        });
        recordAppState('window-message');
      });
    }
  }

  function resizeEntryBrief(entry) {
    const rect = entry?.contentRect;
    return {
      target: elementBrief(entry?.target),
      contentRect: rect
        ? {
            width: Math.round(Number(rect.width) || 0),
            height: Math.round(Number(rect.height) || 0),
          }
        : null,
      borderBoxSize: Array.isArray(entry?.borderBoxSize)
        ? entry.borderBoxSize.slice(0, 2).map((box) => ({
            inlineSize: Math.round(Number(box?.inlineSize) || 0),
            blockSize: Math.round(Number(box?.blockSize) || 0),
          }))
        : undefined,
    };
  }

  function intersectionEntryBrief(entry) {
    return {
      target: elementBrief(entry?.target),
      isIntersecting: !!entry?.isIntersecting,
      intersectionRatio: Number(entry?.intersectionRatio || 0),
    };
  }

  function installObserverApiTrace() {
    const NativeResizeObserver = window.ResizeObserver;
    if (typeof NativeResizeObserver === 'function' && !NativeResizeObserver.__lmChatGPTLiveTraceWrapped) {
      function TracedResizeObserver(callback) {
        observerApiStats.resizeConstructed += 1;
        record('resize-observer-create');
        const wrapped = function tracedResizeObserverCallback(entries, observer) {
          observerApiStats.resizeCallback += 1;
          observerApiStats.resizeEntryCount += entries?.length || 0;
          observerApiStats.lastResizeCallbackAt = nowMs();
          record('resize-observer-callback', {
            count: entries?.length || 0,
            entries: [...entries || []].slice(0, 8).map(resizeEntryBrief),
          });
          recordAppState('resize-observer-callback');
          return callback.apply(this, arguments);
        };
        return new NativeResizeObserver(wrapped);
      }
      TracedResizeObserver.prototype = NativeResizeObserver.prototype;
      Object.setPrototypeOf(TracedResizeObserver, NativeResizeObserver);
      TracedResizeObserver.__lmChatGPTLiveTraceWrapped = true;
      window.ResizeObserver = TracedResizeObserver;

      const nativeObserve = NativeResizeObserver.prototype.observe;
      if (typeof nativeObserve === 'function' && !nativeObserve.__lmChatGPTLiveTraceWrapped) {
        NativeResizeObserver.prototype.observe = function tracedResizeObserverObserve(target, options) {
          observerApiStats.resizeObserve += 1;
          record('resize-observer-observe', {target: elementBrief(target), box: String(options?.box || '')});
          return nativeObserve.apply(this, arguments);
        };
        NativeResizeObserver.prototype.observe.__lmChatGPTLiveTraceWrapped = true;
      }
      const nativeUnobserve = NativeResizeObserver.prototype.unobserve;
      if (typeof nativeUnobserve === 'function' && !nativeUnobserve.__lmChatGPTLiveTraceWrapped) {
        NativeResizeObserver.prototype.unobserve = function tracedResizeObserverUnobserve(target) {
          observerApiStats.resizeUnobserve += 1;
          return nativeUnobserve.apply(this, arguments);
        };
        NativeResizeObserver.prototype.unobserve.__lmChatGPTLiveTraceWrapped = true;
      }
      const nativeDisconnect = NativeResizeObserver.prototype.disconnect;
      if (typeof nativeDisconnect === 'function' && !nativeDisconnect.__lmChatGPTLiveTraceWrapped) {
        NativeResizeObserver.prototype.disconnect = function tracedResizeObserverDisconnect() {
          observerApiStats.resizeDisconnect += 1;
          return nativeDisconnect.apply(this, arguments);
        };
        NativeResizeObserver.prototype.disconnect.__lmChatGPTLiveTraceWrapped = true;
      }
    }

    const NativeIntersectionObserver = window.IntersectionObserver;
    if (typeof NativeIntersectionObserver === 'function' && !NativeIntersectionObserver.__lmChatGPTLiveTraceWrapped) {
      function TracedIntersectionObserver(callback, options) {
        observerApiStats.intersectionConstructed += 1;
        record('intersection-observer-create', {
          root: elementBrief(options?.root),
          rootMargin: String(options?.rootMargin || ''),
        });
        const wrapped = function tracedIntersectionObserverCallback(entries, observer) {
          observerApiStats.intersectionCallback += 1;
          observerApiStats.intersectionEntryCount += entries?.length || 0;
          observerApiStats.lastIntersectionCallbackAt = nowMs();
          record('intersection-observer-callback', {
            count: entries?.length || 0,
            entries: [...entries || []].slice(0, 8).map(intersectionEntryBrief),
          });
          recordAppState('intersection-observer-callback');
          return callback.apply(this, arguments);
        };
        return new NativeIntersectionObserver(wrapped, options);
      }
      TracedIntersectionObserver.prototype = NativeIntersectionObserver.prototype;
      Object.setPrototypeOf(TracedIntersectionObserver, NativeIntersectionObserver);
      TracedIntersectionObserver.__lmChatGPTLiveTraceWrapped = true;
      window.IntersectionObserver = TracedIntersectionObserver;

      const nativeObserve = NativeIntersectionObserver.prototype.observe;
      if (typeof nativeObserve === 'function' && !nativeObserve.__lmChatGPTLiveTraceWrapped) {
        NativeIntersectionObserver.prototype.observe = function tracedIntersectionObserverObserve(target) {
          observerApiStats.intersectionObserve += 1;
          record('intersection-observer-observe', {target: elementBrief(target)});
          return nativeObserve.apply(this, arguments);
        };
        NativeIntersectionObserver.prototype.observe.__lmChatGPTLiveTraceWrapped = true;
      }
      const nativeUnobserve = NativeIntersectionObserver.prototype.unobserve;
      if (typeof nativeUnobserve === 'function' && !nativeUnobserve.__lmChatGPTLiveTraceWrapped) {
        NativeIntersectionObserver.prototype.unobserve = function tracedIntersectionObserverUnobserve(target) {
          observerApiStats.intersectionUnobserve += 1;
          return nativeUnobserve.apply(this, arguments);
        };
        NativeIntersectionObserver.prototype.unobserve.__lmChatGPTLiveTraceWrapped = true;
      }
      const nativeDisconnect = NativeIntersectionObserver.prototype.disconnect;
      if (typeof nativeDisconnect === 'function' && !nativeDisconnect.__lmChatGPTLiveTraceWrapped) {
        NativeIntersectionObserver.prototype.disconnect = function tracedIntersectionObserverDisconnect() {
          observerApiStats.intersectionDisconnect += 1;
          return nativeDisconnect.apply(this, arguments);
        };
        NativeIntersectionObserver.prototype.disconnect.__lmChatGPTLiveTraceWrapped = true;
      }
    }
  }

  function recordInterestingInsert(kind, parent, child) {
    if (!isInterestingDomNode(parent) && !isInterestingDomNode(child)) return;
    domMutationStats.interestingInserts += 1;
    domMutationStats.lastInterestingMutationAt = nowMs();
    const sample = {
      time: nowMs(),
      kind,
      parent: nodeBrief(parent),
      child: nodeBrief(child),
    };
    pushDomMutationSample(sample);
    record('dom-insert', sample);
  }

  function installDomMutationTrace() {
    const NativeMutationObserver = window.MutationObserver;
    if (typeof NativeMutationObserver === 'function' && !window.__lmChatGPTLiveTraceMutationObserver) {
      try {
        const observer = new NativeMutationObserver((records) => {
          domMutationStats.mutationObserverRecords += records.length;
          const interesting = records.filter((record) => {
            const childHit = [...record.addedNodes || [], ...record.removedNodes || []].some(isInterestingDomNode);
            return childHit || isInterestingDomNode(record.target);
          });
          if (!interesting.length) return;
          domMutationStats.mutationObserverInteresting += interesting.length;
          domMutationStats.lastInterestingMutationAt = nowMs();
          for (const record of interesting) {
            domMutationStats.interestingInserts += [...record.addedNodes || []].filter(isInterestingDomNode).length;
            domMutationStats.interestingRemovals += [...record.removedNodes || []].filter(isInterestingDomNode).length;
          }
          const sample = {
            time: nowMs(),
            kind: 'MutationObserver',
            count: records.length,
            interestingCount: interesting.length,
            records: interesting.slice(0, 10).map(mutationRecordBrief),
          };
          pushDomMutationSample(sample);
          record('dom-mutation', sample);
          recordDomState('dom-mutation');
        });
        observer.observe(document.documentElement || document, {
          childList: true,
          subtree: true,
          attributes: true,
          attributeFilter: [
            'data-turn',
            'data-message-id',
            'data-message-author-role',
            'data-testid',
            'role',
            'class',
            'id',
          ],
        });
        window.__lmChatGPTLiveTraceMutationObserver = observer;
      } catch {}
    }

    const nativeAppendChild = Node.prototype.appendChild;
    if (typeof nativeAppendChild === 'function' && !nativeAppendChild.__lmChatGPTLiveTraceWrapped) {
      Node.prototype.appendChild = function tracedAppendChild(child) {
        domMutationStats.appendChildCalls += 1;
        recordInterestingInsert('appendChild', this, child);
        return nativeAppendChild.apply(this, arguments);
      };
      Node.prototype.appendChild.__lmChatGPTLiveTraceWrapped = true;
    }

    const nativeInsertBefore = Node.prototype.insertBefore;
    if (typeof nativeInsertBefore === 'function' && !nativeInsertBefore.__lmChatGPTLiveTraceWrapped) {
      Node.prototype.insertBefore = function tracedInsertBefore(child) {
        domMutationStats.insertBeforeCalls += 1;
        recordInterestingInsert('insertBefore', this, child);
        return nativeInsertBefore.apply(this, arguments);
      };
      Node.prototype.insertBefore.__lmChatGPTLiveTraceWrapped = true;
    }

    const nativeReplaceChildren = Element.prototype.replaceChildren;
    if (typeof nativeReplaceChildren === 'function' && !nativeReplaceChildren.__lmChatGPTLiveTraceWrapped) {
      Element.prototype.replaceChildren = function tracedReplaceChildren(...children) {
        domMutationStats.replaceChildrenCalls += 1;
        for (const child of children) recordInterestingInsert('replaceChildren', this, child);
        return nativeReplaceChildren.apply(this, arguments);
      };
      Element.prototype.replaceChildren.__lmChatGPTLiveTraceWrapped = true;
    }
  }

  function sourceSummary(info) {
    if (!info) return null;
    return {
      url: info.url,
      method: info.method,
      status: info.status,
      ok: info.ok,
    };
  }

  function sourceKey(info) {
    if (!info) return '';
    return `${info.method || ''} ${info.url || ''}`;
  }

  function sourceStat(info) {
    const key = sourceKey(info);
    if (!key) return null;
    let stat = sourceStats.get(key);
    if (!stat) {
      stat = {
        source: sourceSummary(info),
        fetchResponses: 0,
        responseClones: 0,
        bodyReads: {},
        getReaders: 0,
        streamReads: 0,
        streamDoneReads: 0,
        totalBytes: 0,
        lastChunk: null,
        lastDataChunk: null,
        frameStats: {
          jsonValues: 0,
          dataLines: 0,
          doneLines: 0,
          typeCounts: {},
          patchCount: 0,
          patchPathCounts: {},
          patchOpCounts: {},
          patchFieldKindCounts: {},
          patchValueKeySetCounts: {},
          keySetCounts: {},
          roleCounts: {},
          stringPathCounts: {},
          errorEventCount: 0,
          errorCodeCounts: {},
          errorFieldShapes: {},
          errorMessageShapes: {},
          patchSamples: [],
        },
      };
      sourceStats.set(key, stat);
    }
    return stat;
  }

  function incrementSourceBodyRead(info, methodName) {
    const stat = sourceStat(info);
    if (!stat) return;
    stat.bodyReads[methodName] = (stat.bodyReads[methodName] || 0) + 1;
  }

  function recordSourceChunk(info, done, chunk) {
    const stat = sourceStat(info);
    if (!stat) return;
    stat.streamReads += 1;
    if (done) stat.streamDoneReads += 1;
    const byteLength = Number(chunk?.byteLength || chunk?.size || 0);
    if (Number.isFinite(byteLength)) stat.totalBytes += byteLength;
    stat.lastChunk = chunk;
    if (!done) stat.lastDataChunk = chunk;
    if (chunk?.textFrames?.stats) mergeFrameStats(stat.frameStats, chunk.textFrames.stats);
  }

  function sourceStatsSnapshot() {
    return [...sourceStats.values()]
      .filter((stat) => stat?.source?.url && isInterestingUrl(stat.source.url))
      .slice(-40);
  }

  function isConversationStreamSource(source) {
    const url = String(source?.url || '');
    return url.includes('chatgpt.com/backend-api/f/conversation');
  }

  function recordConversationStreamAppState(source, done, chunk) {
    if (!isConversationStreamSource(source)) return;
    const hasPatch = Number(chunk?.textFrames?.stats?.patchCount || 0) > 0;
    recordAppState(done ? 'conversation-stream-done' : 'conversation-stream-chunk');
    window.queueMicrotask?.(() => recordConversationMaterializationSnapshot(
      done ? 'stream-read-done-microtask' : 'stream-read-chunk-microtask',
    ));
    if (hasPatch) {
      window.queueMicrotask?.(() => recordAppState('conversation-stream-patch-microtask'));
      window.setTimeout?.(() => {
        recordAppState('conversation-stream-patch-timeout');
        recordConversationMaterializationSnapshot(
          done ? 'stream-read-done-timeout' : 'stream-read-patch-timeout',
        );
      }, 0);
    }
  }

  function incrementCount(target, key, amount = 1, maxKeys = 80) {
    if (!key) return;
    if (!Object.prototype.hasOwnProperty.call(target, key) && Object.keys(target).length >= maxKeys) {
      key = '<other>';
    }
    target[key] = (target[key] || 0) + amount;
  }

  function mergeCountMap(target, source) {
    if (!source || typeof source !== 'object') return;
    for (const [key, value] of Object.entries(source)) {
      incrementCount(target, key, Number(value) || 0);
    }
  }

  function mergeFrameStats(target, source) {
    if (!source || typeof source !== 'object') return;
    target.jsonValues += Number(source.jsonValues || 0);
    target.dataLines += Number(source.dataLines || 0);
    target.doneLines += Number(source.doneLines || 0);
    target.patchCount += Number(source.patchCount || 0);
    mergeCountMap(target.typeCounts, source.typeCounts);
    mergeCountMap(target.patchPathCounts, source.patchPathCounts);
    mergeCountMap(target.patchOpCounts, source.patchOpCounts);
    mergeCountMap(target.patchFieldKindCounts, source.patchFieldKindCounts);
    mergeCountMap(target.patchValueKeySetCounts, source.patchValueKeySetCounts);
    mergeCountMap(target.keySetCounts, source.keySetCounts);
    mergeCountMap(target.roleCounts, source.roleCounts);
    mergeCountMap(target.stringPathCounts, source.stringPathCounts);
    target.errorEventCount += Number(source.errorEventCount || 0);
    mergeCountMap(target.errorCodeCounts, source.errorCodeCounts);
    mergeCountMap(target.errorFieldShapes, source.errorFieldShapes);
    mergeCountMap(target.errorMessageShapes, source.errorMessageShapes);
    if (Array.isArray(source.patchSamples)) {
      for (const sample of source.patchSamples) {
        addPatchSample(target.patchSamples, sample);
      }
    }
  }

  function safePathSegment(segment) {
    if (typeof segment === 'number') return '#';
    if (typeof segment !== 'string') return typeof segment;
    if (segment === '#') return '#';
    if (/^\d+$/.test(segment)) return '#';
    if (/^[0-9a-f]{8,}$/i.test(segment) || /^[0-9a-f-]{16,}$/i.test(segment)) {
      return '<id>';
    }
    if (/^[A-Za-z_$][A-Za-z0-9_$-]{0,32}$/.test(segment)) return segment;
    return `<str:${segment.length}>`;
  }

  function patchPathStringShape(path) {
    const raw = String(path || '');
    if (!raw) return '<str:0>';
    const parts = raw.split(/[/.]+/).filter(Boolean);
    if (parts.length <= 1) return safePathSegment(raw);
    return parts.slice(0, 12).map(safePathSegment).join('.');
  }

  function patchPathShape(path) {
    if (typeof path === 'string') return patchPathStringShape(path);
    if (typeof path === 'number') return '#';
    if (!Array.isArray(path)) return '';
    return path.slice(0, 12).map(safePathSegment).join('.');
  }

  function safeScalarToken(value) {
    if (typeof value === 'string') {
      if (/^[A-Za-z0-9_$:.-]{1,32}$/.test(value)) return value;
      return `string:${value.length}`;
    }
    if (typeof value === 'number' || typeof value === 'boolean') return String(value);
    if (value === null) return 'null';
    return Array.isArray(value) ? 'array' : typeof value;
  }

  function shapeKind(value) {
    if (Array.isArray(value)) return `array:${value.length}`;
    if (value === null) return 'null';
    if (ArrayBuffer.isView(value)) return value.constructor?.name || 'typedarray';
    if (value && typeof value === 'object') return 'object';
    if (typeof value === 'string') return `string:${value.length}`;
    return typeof value;
  }

  function sortedKeyShape(value) {
    if (!value || typeof value !== 'object' || Array.isArray(value)) return '';
    return Object.keys(value).sort().slice(0, 8).join(',');
  }

  function isInterestingPatchSample(sample) {
    const valueKind = sample?.fieldKinds?.v || '';
    return (
      valueKind.startsWith('string:') ||
      valueKind.startsWith('array:') ||
      sample?.op === 'append' ||
      sample?.op === 'replace'
    );
  }

  function addPatchSample(samples, sample) {
    if (!sample) return;
    sample.interesting = isInterestingPatchSample(sample);
    if (samples.length < 8) {
      samples.push(sample);
      return;
    }
    if (!sample.interesting) return;
    const replaceIndex = samples.findIndex((existing) => !existing?.interesting);
    if (replaceIndex >= 0) samples[replaceIndex] = sample;
  }

  function summarizeJsonFrame(value, stats, path = [], depth = 0) {
    stats.jsonValues += 1;
    if (depth > 6) return;
    if (Array.isArray(value)) {
      for (const item of value.slice(0, 80)) summarizeJsonFrame(item, stats, path.concat('#'), depth + 1);
      return;
    }
    if (!value || typeof value !== 'object') {
      if (typeof value === 'string') {
        incrementCount(stats.stringPathCounts, `${patchPathShape(path) || '<root>'}:len${value.length}`);
      }
      return;
    }
    const keys = Object.keys(value).sort();
    incrementCount(stats.keySetCounts, keys.slice(0, 8).join(','));
    if (typeof value.type === 'string') incrementCount(stats.typeCounts, value.type);
    if (typeof value.role === 'string') incrementCount(stats.roleCounts, safeScalarToken(value.role));
    if (value.author && typeof value.author.role === 'string') {
      incrementCount(stats.roleCounts, safeScalarToken(value.author.role));
    }
    if (
      Object.prototype.hasOwnProperty.call(value, 'error') ||
      Object.prototype.hasOwnProperty.call(value, 'error_code')
    ) {
      stats.errorEventCount += 1;
      if (typeof value.error_code === 'string') {
        incrementCount(stats.errorCodeCounts, safeScalarToken(value.error_code));
      }
      if (Object.prototype.hasOwnProperty.call(value, 'error')) {
        incrementCount(stats.errorFieldShapes, shapeKind(value.error));
      }
      if (Object.prototype.hasOwnProperty.call(value, 'message')) {
        incrementCount(stats.errorFieldShapes, `message:${shapeKind(value.message)}`);
      }
      if (typeof value.error === 'string') {
        incrementCount(stats.errorMessageShapes, `error:len${value.error.length}`);
      }
      if (typeof value.message === 'string') {
        incrementCount(stats.errorMessageShapes, `message:len${value.message.length}`);
      }
    }
    if (
      Object.prototype.hasOwnProperty.call(value, 'p') ||
      Object.prototype.hasOwnProperty.call(value, 'o') ||
      Object.prototype.hasOwnProperty.call(value, 'v') ||
      Object.prototype.hasOwnProperty.call(value, 'c')
    ) {
      stats.patchCount += 1;
      const patchPath = patchPathShape(value.p) || patchPathShape(path) || '<missing>';
      incrementCount(stats.patchPathCounts, patchPath);
      if (Object.prototype.hasOwnProperty.call(value, 'o')) {
        incrementCount(stats.patchOpCounts, safeScalarToken(value.o));
      }
      for (const field of ['p', 'o', 'v', 'c']) {
        if (Object.prototype.hasOwnProperty.call(value, field)) {
          incrementCount(stats.patchFieldKindCounts, `${field}:${shapeKind(value[field])}`);
        }
      }
      const valueKeyShape = sortedKeyShape(value.v);
      if (valueKeyShape) incrementCount(stats.patchValueKeySetCounts, valueKeyShape);
      const fieldKinds = {};
      for (const field of ['p', 'o', 'v', 'c']) {
        if (Object.prototype.hasOwnProperty.call(value, field)) {
          fieldKinds[field] = shapeKind(value[field]);
        }
      }
      addPatchSample(stats.patchSamples, {
        path: patchPath,
        keys: keys.slice(0, 8),
        op: Object.prototype.hasOwnProperty.call(value, 'o') ? safeScalarToken(value.o) : '',
        fieldKinds,
        valueShape: valueShape(value.v),
        containerShape: valueShape(value.c),
      });
    }
    for (const key of keys.slice(0, 40)) {
      summarizeJsonFrame(value[key], stats, path.concat(key), depth + 1);
    }
  }

  function summarizeTextFrames(text) {
    const allLines = String(text || '').split(/\r?\n/).filter(Boolean);
    const lines = allLines.slice(0, 30);
    const dataShapes = [];
    const stats = {
      jsonValues: 0,
      dataLines: 0,
      doneLines: 0,
      typeCounts: {},
      patchCount: 0,
      patchPathCounts: {},
      patchOpCounts: {},
      patchFieldKindCounts: {},
      patchValueKeySetCounts: {},
      keySetCounts: {},
      roleCounts: {},
      stringPathCounts: {},
      errorEventCount: 0,
      errorCodeCounts: {},
      errorFieldShapes: {},
      errorMessageShapes: {},
      patchSamples: [],
    };
    for (const line of allLines) {
      const trimmed = line.trim();
      const payload = trimmed.startsWith('data:') ? trimmed.slice(5).trim() : trimmed;
      if (!payload) continue;
      if (trimmed.startsWith('data:')) stats.dataLines += 1;
      if (payload === '[DONE]') {
        stats.doneLines += 1;
        continue;
      }
      if (!payload.startsWith('{') && !payload.startsWith('[')) continue;
      try {
        const parsed = JSON.parse(payload);
        summarizeJsonFrame(parsed, stats);
        if (dataShapes.length < 8 && lines.includes(line)) dataShapes.push(valueShape(parsed));
      } catch {}
    }
    return {
      textLength: String(text || '').length,
      lineCount: allLines.length,
      sampledLineCount: lines.length,
      dataLineCount: stats.dataLines,
      doneCount: stats.doneLines,
      stats,
      dataShapes,
    };
  }

  const REQUEST_BODY_MARKERS = [
    'local_conversation_event',
    'missing_existing_node',
    'missing_expected_parent',
    'skipped_missing_parent',
    'inserted_without_parent',
    'inserted',
    'existing_message_update_queued',
    'existing_message_updated',
    'conversation.input_missing_expected_parent',
    'chatgpt_web_conversation_tree_node_not_found',
    'getNodeByIdOrMessageId',
    'streamError',
    'messageReceived',
    'assistantMessageReceived',
    'completionFinished',
    'Attribution: Client Thread to Server Thread',
    'Create New Thread',
    'Init new thread',
    'conversation.server_overwrite_stale',
    'chatgpt_web_conversation_server_overwrite_stale',
  ];

  function markerCounts(text) {
    const counts = {};
    const source = String(text || '');
    for (const marker of REQUEST_BODY_MARKERS) {
      let count = 0;
      let index = source.indexOf(marker);
      while (index !== -1) {
        count += 1;
        index = source.indexOf(marker, index + marker.length);
      }
      if (count) counts[marker] = count;
    }
    return counts;
  }

  function hasMarkers(counts) {
    return !!counts && Object.keys(counts).length > 0;
  }

  function summarizeRequestBody(body) {
    if (body == null) return {kind: 'none'};
    if (typeof body === 'string') {
      const sample = body.slice(0, 120000);
      return {
        kind: 'string',
        length: body.length,
        markers: markerCounts(sample),
        chatRequest: summarizeChatRequestText(sample),
      };
    }
    if (body instanceof URLSearchParams) {
      const text = body.toString();
      return {
        kind: 'URLSearchParams',
        length: text.length,
        keys: [...body.keys()].slice(0, 40),
        markers: markerCounts(text.slice(0, 120000)),
        chatRequest: summarizeChatRequestText(text.slice(0, 120000)),
      };
    }
    if (body instanceof FormData) {
      const keys = [];
      try {
        for (const key of body.keys()) keys.push(String(key));
      } catch {}
      return {kind: 'FormData', keys: keys.slice(0, 40), markers: {}};
    }
    if (body instanceof Blob) {
      return {kind: 'Blob', size: body.size, type: body.type || '', markers: {}};
    }
    if (body instanceof ArrayBuffer || ArrayBuffer.isView(body)) {
      const byteLength = body.byteLength || 0;
      let markers = {};
      try {
        const view = body instanceof ArrayBuffer
          ? new Uint8Array(body)
          : new Uint8Array(body.buffer, body.byteOffset, body.byteLength);
        const text = new TextDecoder('utf-8', {fatal: false}).decode(
          view.slice(0, Math.min(view.byteLength, 120000))
        );
        markers = markerCounts(text);
        return {
          kind: Object.prototype.toString.call(body).slice(8, -1),
          byteLength,
          markers,
          chatRequest: summarizeChatRequestText(text),
        };
      } catch {}
      return {kind: Object.prototype.toString.call(body).slice(8, -1), byteLength, markers};
    }
    return {kind: Object.prototype.toString.call(body), keys: objectKeys(body, 20), markers: {}};
  }

  function idShape(value) {
    if (typeof value !== 'string' || !value) return undefined;
    const match = value.match(/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}/i);
    if (match) return match[0].slice(0, 8) + '...' + match[0].slice(-4);
    if (/^[A-Za-z0-9_-]{12,}$/.test(value)) return value.slice(0, 6) + '...' + value.slice(-4);
    return `string:${value.length}`;
  }

  function messageRequestShape(message) {
    if (!message || typeof message !== 'object') return {kind: typeof message};
    const parts = Array.isArray(message.content?.parts) ? message.content.parts : [];
    return {
      id: idShape(message.id),
      role: typeof message.author?.role === 'string' ? message.author.role : '',
      recipient: typeof message.recipient === 'string' ? message.recipient : '',
      parent: idShape(message.metadata?.parent_id),
      requestId: idShape(message.metadata?.request_id),
      contentType: typeof message.content?.content_type === 'string' ? message.content.content_type : '',
      partLengths: parts.slice(0, 6).map((part) => String(part || '').length),
    };
  }

  function summarizeChatRequestText(text) {
    if (!text || !String(text).trim().startsWith('{')) return undefined;
    try {
      const data = JSON.parse(text);
      if (!data || typeof data !== 'object') return undefined;
      return {
        keys: objectKeys(data, 40),
        action: typeof data.action === 'string' ? data.action : '',
        conversationId: idShape(data.conversation_id),
        threadId: idShape(data.thread_id ?? data.threadId),
        parentMessageId: idShape(data.parent_message_id ?? data.parentMessageId),
        forceParagen: data.force_paragen === true,
        messages: Array.isArray(data.messages)
          ? data.messages.slice(0, 8).map(messageRequestShape)
          : [],
      };
    } catch {
      return undefined;
    }
  }

  function installRequestConstructorTrace() {
    const NativeRequest = window.Request;
    if (typeof NativeRequest !== 'function' || NativeRequest.__lmChatGPTLiveTraceWrapped) return;
    function TracedRequest(input, init) {
      if (!new.target) {
        throw new TypeError("Failed to construct 'Request': Please use the 'new' operator.");
      }
      const request = new NativeRequest(input, init);
      try {
        const rawUrl = request.url || input?.url || input;
        const method = String(request.method || init?.method || input?.method || 'GET').toUpperCase();
        if (isConversationPostUrl(rawUrl)) {
          record('request-create', {
            url: compactUrl(rawUrl),
            method,
            inputKind: Object.prototype.toString.call(input),
            body: summarizeRequestBody(init?.body),
          });
        }
      } catch {}
      return request;
    }
    TracedRequest.prototype = NativeRequest.prototype;
    Object.setPrototypeOf(TracedRequest, NativeRequest);
    TracedRequest.__lmChatGPTLiveTraceWrapped = true;
    window.Request = TracedRequest;
  }

  function summarizeStreamChunk(value, source) {
    const base = summarizeData(value);
    if (!source || !isInterestingUrl(source.url)) return base;
    let text = '';
    try {
      if (typeof value === 'string') {
        text = value;
      } else if (value instanceof ArrayBuffer && typeof TextDecoder !== 'undefined') {
        text = new TextDecoder('utf-8', {fatal: false}).decode(new Uint8Array(value));
      } else if (ArrayBuffer.isView(value) && typeof TextDecoder !== 'undefined') {
        text = new TextDecoder('utf-8', {fatal: false}).decode(value);
      }
    } catch {}
    if (!text) return base;
    return {...base, textFrames: summarizeTextFrames(text)};
  }

  const nativeFetch = window.fetch;
  if (typeof nativeFetch === 'function') {
    window.fetch = function tracedFetch(input, init) {
      const rawUrl = input?.url || input;
      const url = compactUrl(rawUrl);
      const method = String(init?.method || input?.method || 'GET').toUpperCase();
      const interesting = isInterestingUrl(rawUrl);
      const conversationPost = isConversationPostUrl(rawUrl);
      if (isTelemetryUrl(rawUrl)) {
        const bodySummary = summarizeRequestBody(init?.body);
        if (hasMarkers(bodySummary.markers) || bodySummary.kind !== 'none') {
          record('fetch-request-body', {url, method, body: bodySummary});
        }
      } else if (conversationPost) {
        const bodySummary = summarizeRequestBody(init?.body);
        record('fetch-request-body', {url, method, body: bodySummary});
        if (bodySummary.kind === 'none' && input instanceof Request) {
          try {
            input.clone().text().then((text) => {
              record('fetch-request-body', {
                url,
                method,
                body: {
                  kind: 'RequestCloneText',
                  length: text.length,
                  markers: markerCounts(text.slice(0, 120000)),
                  chatRequest: summarizeChatRequestText(text.slice(0, 120000)),
                },
              });
            }).catch(() => {});
          } catch {}
        }
      }
      if (interesting) record('fetch-start', {url, method});
      return nativeFetch.apply(this, arguments).then((response) => {
        const responseUrl = compactUrl(response?.url || rawUrl);
        if (interesting || isInterestingUrl(response?.url)) {
          const info = {
            url: responseUrl,
            method,
            status: response.status,
            ok: !!response.ok,
          };
          responseInfos.set(response, info);
          if (response?.body) streamInfos.set(response.body, info);
          sourceStat(info).fetchResponses += 1;
          record('fetch-response', {
            url: responseUrl,
            method,
            status: response.status,
            ok: !!response.ok,
            redirected: !!response.redirected,
            hasBody: !!response.body,
          });
        }
        return response;
      }, (error) => {
        if (interesting) {
          record('fetch-error', {url, method, name: error?.name || '', message: String(error?.message || error).slice(0, 240)});
        }
        throw error;
      });
    };
  }
  if (window.Response?.prototype?.clone) {
    const nativeResponseClone = window.Response.prototype.clone;
    window.Response.prototype.clone = function tracedResponseClone(...args) {
      const cloned = nativeResponseClone.apply(this, args);
      const info = responseInfos.get(this);
      if (info) {
        responseInfos.set(cloned, info);
        if (cloned?.body) streamInfos.set(cloned.body, info);
        sourceStat(info).responseClones += 1;
        record('response-clone', {source: sourceSummary(info), hasBody: !!cloned?.body});
      }
      return cloned;
    };
  }
  for (const methodName of ['arrayBuffer', 'blob', 'formData', 'json', 'text']) {
    const nativeMethod = window.Response?.prototype?.[methodName];
    if (typeof nativeMethod !== 'function') continue;
    window.Response.prototype[methodName] = function tracedBodyMethod(...args) {
      const info = responseInfos.get(this);
      if (info && isInterestingUrl(info.url)) {
        incrementSourceBodyRead(info, methodName);
        record('response-body-read', {source: sourceSummary(info), method: methodName});
      }
      return nativeMethod.apply(this, args);
    };
  }

  const nativeWebSocket = window.WebSocket;
  if (typeof nativeWebSocket === 'function') {
    function TracedWebSocket(url, protocols) {
      const compact = compactUrl(url);
      const interesting = isInterestingUrl(url);
      if (interesting) record('ws-create', {url: compact});
      const ws = protocols === undefined ? new nativeWebSocket(url) : new nativeWebSocket(url, protocols);
      if (interesting) {
        ws.addEventListener('open', () => record('ws-open', {url: compact}));
        ws.addEventListener('message', (event) => {
          record('ws-message', {url: compact, data: summarizeData(event.data)});
          recordAppState('ws-message');
          window.queueMicrotask?.(() => recordAppState('ws-message-microtask'));
          window.setTimeout?.(() => recordAppState('ws-message-timeout'), 0);
        });
        ws.addEventListener('error', () => record('ws-error', {url: compact}));
        ws.addEventListener('close', (event) => record('ws-close', {url: compact, code: event.code, reasonLen: String(event.reason || '').length}));
      }
      return ws;
    }
    TracedWebSocket.prototype = nativeWebSocket.prototype;
    Object.setPrototypeOf(TracedWebSocket, nativeWebSocket);
    for (const name of ['CONNECTING', 'OPEN', 'CLOSING', 'CLOSED']) {
      try {
        Object.defineProperty(TracedWebSocket, name, {value: nativeWebSocket[name]});
      } catch {}
    }
    window.WebSocket = TracedWebSocket;
  }

  if (window.ReadableStream?.prototype?.getReader) {
    const nativeGetReader = window.ReadableStream.prototype.getReader;
    window.ReadableStream.prototype.getReader = function tracedGetReader(...args) {
      const source = streamInfos.get(this);
      if (source) sourceStat(source).getReaders += 1;
      record('stream-get-reader', {mode: args[0]?.mode || '', source: sourceSummary(source)});
      const reader = nativeGetReader.apply(this, args);
      if (reader?.read && !reader.__lmChatGPTLiveTraceReadWrapped) {
        const nativeRead = reader.read.bind(reader);
        reader.read = function tracedReaderRead(...readArgs) {
          return nativeRead(...readArgs).then((result) => {
            const chunk = summarizeStreamChunk(result.value, source);
            recordSourceChunk(source, !!result.done, chunk);
            record('stream-read', {
              done: !!result.done,
              source: sourceSummary(source),
              chunk,
            });
            recordConversationStreamAppState(source, !!result.done, chunk);
            return result;
          });
        };
        reader.__lmChatGPTLiveTraceReadWrapped = true;
      }
      return reader;
    };
  }
  if (window.TextDecoderStream) {
    const NativeTextDecoderStream = window.TextDecoderStream;
    window.TextDecoderStream = function tracedTextDecoderStream(...args) {
      record('text-decoder-stream', {encoding: args[0] || 'utf-8'});
      return new NativeTextDecoderStream(...args);
    };
    window.TextDecoderStream.prototype = NativeTextDecoderStream.prototype;
    Object.setPrototypeOf(window.TextDecoderStream, NativeTextDecoderStream);
  }

  installReactCommitTrace();
  installEventLoopTrace();
  installMessageTaskTrace();
  installObserverApiTrace();
  installDomMutationTrace();
  installRequestConstructorTrace();
  installHistoryTrace();
  installNavigationApiTrace();
  installIdMapTrace();

  if (document.documentElement) {
    installMutationTrace();
  } else {
    document.addEventListener('DOMContentLoaded', installMutationTrace, {once: true});
  }

  window.__lmChatGPTLiveTraceSnapshot = function liveTraceSnapshot() {
    const snapshot = {snapshotErrors: []};
    function field(name, producer) {
      try {
        snapshot[name] = producer();
      } catch (error) {
        snapshot.snapshotErrors.push({
          field: name,
          name: String(error?.name || ''),
          message: String(error?.message || error).slice(0, 240),
        });
      }
    }
    field('domStateRecord', () => {
      recordDomState('snapshot');
      return true;
    });
    field('url', () => compactUrl(location.href));
    field('state', () => conversationDomState());
    field('eventLoop', () => eventLoopState());
    field('messageTasks', () => messageTaskState());
    field('domMutations', () => domMutationState());
    field('reactCommits', () => reactCommitState());
    field('reactConversationWrappers', () => currentConversationWrapperSnapshots());
    field('conversationMaterialization', () => conversationMaterializationTraceState());
    field('conversationIdentityTrace', () => conversationIdentityTraceState());
    field('reactThreadFiber', () => reactThreadFiberSnapshot());
    field('threadRendererProbes', () => threadRendererProbes());
    field('suspenseBoundaryProbes', () => suspenseBoundaryProbes());
    field('reactionStoreProbes', () => reactionStoreProbes());
    field('threadStoreHooks', () => threadStoreHookSnapshots());
    field('navigationApi', () => navigationTraceState());
    field('idMapTrace', () => idMapTraceState());
    field('sourceStats', () => sourceStatsSnapshot());
    field('events', () => events.slice(-240));
    return snapshot;
  };
})();
"""


def looks_like_navigation_context_loss(error: BaseException) -> bool:
    text = str(error) or repr(error)
    return (
        "Execution context was destroyed" in text
        or "Cannot find context with specified id" in text
        or "Most likely because of a navigation" in text
    )


def looks_like_submit_timeout(error: BaseException) -> bool:
    text = str(error) or repr(error)
    return "timed out" in text and (
        "fillEmail" in text or "fillPassword" in text or "Page.evaluate" in text
    )


def auth_blocking_reason_from_url(url: str) -> str:
    lowered = url.lower()
    if "auth.openai.com/email-verification" in lowered:
        return "email-verification"
    return ""


def is_auth_password_url(url: str) -> bool:
    return "auth.openai.com/log-in/password" in url.lower()


def is_auth_password_missing_session_state(state: dict[str, Any]) -> bool:
    if state.get("hasPasswordInput"):
        return False
    text = str(state.get("text") or "").lower()
    return (
        "oops, an error occurred" in text
        and (
            "missing already entered username" in text
            or "unknown error" in text
            or "is_missing_session" in text
        )
    )


def is_actionable_auth_password_state(state: dict[str, Any]) -> bool:
    return (
        is_auth_password_url(str(state.get("url") or ""))
        and bool(state.get("hasPasswordInput"))
        and not bool(state.get("blockingReason"))
        and not bool(state.get("loggedIn"))
    )


def is_retryable_auth_password_error_state(state: dict[str, Any]) -> bool:
    if not is_auth_password_url(str(state.get("url") or "")):
        return False
    if state.get("hasPasswordInput") or state.get("loggedIn") or state.get("blockingReason"):
        return False
    text = str(state.get("text") or "").lower()
    return "try again" in text and (
        "operation timed out" in text
        or "oops, an error occurred" in text
        or "something went wrong" in text
    )


def is_waitable_blocking_reason(reason: Any) -> bool:
    return str(reason or "") in WAITABLE_BLOCKING_REASONS


def is_code_blocking_reason(reason: Any) -> bool:
    return str(reason or "") in CODE_BLOCKING_REASONS


def waitable_blocking_error(state: dict[str, Any]) -> DemoError:
    reason = str(state.get("blockingReason") or "auth-step")
    if reason == "device-approval":
        return DemoError(
            "login timed out while waiting for device approval; approve it on the listed device "
            f"or rerun with a longer --login-timeout; state={redact_snapshot(state)!r}"
        )
    return DemoError(f"login is waiting for auth step ({reason}); state={redact_snapshot(state)!r}")


def check_auth_intermediate_url(url: str) -> None:
    reason = auth_blocking_reason_from_url(url)
    if reason:
        raise DemoError(f"login reached an auth verification step ({reason}); this demo does not bypass it")


def is_transient_assistant_text(text: str) -> bool:
    normalized = re.sub(r"\s+", " ", text).strip().lower()
    return (
        normalized in {"", "thinking", "thinking...", "generating", "searching"}
        or normalized.endswith(" thinking")
        or normalized.endswith(" thinking...")
    )


def is_retryable_read_only_helper_error(error: BaseException) -> bool:
    text = str(error) or repr(error)
    return (
        "Playwright helper `" in text
        and ("timed out" in text or "Execution context was destroyed" in text)
    )


def redact_diagnostic_text(text: str) -> str:
    return redact_sensitive_text(text)


def is_diagnostic_url(url: str) -> bool:
    return any(
        host in url
        for host in (
            "openai.com",
            "chatgpt.com",
            "oaistatic.com",
            "sentinel.openai.com",
        )
    )


def is_conversation_diagnostic_url(url: str) -> bool:
    return any(
        marker in url
        for marker in (
            "chatgpt.com/backend-api/f/conversation",
            "chatgpt.com/backend-api/conversation/",
            "chatgpt.com/backend-api/celsius/ws/user",
            "chatgpt.com/backend-api/sentinel/",
            "ws.chatgpt.com/",
        )
    )


def set_cookie_names_from_header(value: str) -> list[str]:
    return sorted(
        {
            match.group(1)
            for match in re.finditer(
                r"(?:^|[\n,]\s*)([A-Za-z0-9_][A-Za-z0-9_.!#$%&'*+^`|~-]*)(?==)",
                value,
            )
        }
    )


def redacted_id_shape(value: Any) -> str | None:
    if not isinstance(value, str) or not value:
        return None
    match = re.search(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}", value, re.I)
    if match:
        raw = match.group(0)
        return f"{raw[:8]}...{raw[-4:]}"
    if re.fullmatch(r"[A-Za-z0-9_-]{12,}", value):
        return f"{value[:6]}...{value[-4:]}"
    return f"string:{len(value)}"


def chat_request_message_shape(message: Any) -> dict[str, Any]:
    if not isinstance(message, dict):
        return {"kind": type(message).__name__}
    content = message.get("content")
    parts = content.get("parts") if isinstance(content, dict) else []
    if not isinstance(parts, list):
        parts = []
    author = message.get("author")
    metadata = message.get("metadata")
    return {
        "id": redacted_id_shape(message.get("id")),
        "role": author.get("role") if isinstance(author, dict) else "",
        "recipient": message.get("recipient") if isinstance(message.get("recipient"), str) else "",
        "contentType": content.get("content_type") if isinstance(content, dict) else "",
        "partLengths": [len(str(part)) for part in parts[:8]],
        "parent": redacted_id_shape(metadata.get("parent_id")) if isinstance(metadata, dict) else None,
        "requestId": redacted_id_shape(metadata.get("request_id")) if isinstance(metadata, dict) else None,
    }


def chat_request_post_data_shape(text: str | None) -> dict[str, Any] | None:
    if not text:
        return None
    stripped = text.strip()
    if not stripped.startswith("{"):
        return {"kind": "text", "length": len(text)}
    try:
        data = json.loads(stripped)
    except json.JSONDecodeError:
        return {"kind": "invalid-json", "length": len(text)}
    if not isinstance(data, dict):
        return {"kind": type(data).__name__, "length": len(text)}
    messages = data.get("messages")
    return {
        "kind": "json",
        "keys": sorted(str(key) for key in data.keys())[:40],
        "action": data.get("action") if isinstance(data.get("action"), str) else "",
        "conversationId": redacted_id_shape(data.get("conversation_id")),
        "parentMessageId": redacted_id_shape(data.get("parent_message_id")),
        "messages": [
            chat_request_message_shape(message)
            for message in (messages if isinstance(messages, list) else [])[:8]
        ],
        "forceParagen": data.get("force_paragen") is True,
    }


def summarize_live_trace(trace: dict[str, Any] | None) -> dict[str, Any] | None:
    if not isinstance(trace, dict):
        return None
    if "error" in trace or "message" in trace:
        return {
            "error": trace.get("error"),
            "message": trace.get("message"),
        }
    raw_events = trace.get("events")
    events = raw_events if isinstance(raw_events, list) else []
    counts: dict[str, int] = {}
    interesting_tail: list[dict[str, Any]] = []
    request_bodies: list[dict[str, Any]] = []
    for event in events:
        if not isinstance(event, dict):
            continue
        event_type = str(event.get("type") or "")
        counts[event_type] = counts.get(event_type, 0) + 1
        include_event = event_type in {
            "fetch-start",
            "fetch-response",
            "fetch-error",
            "ws-create",
            "ws-open",
            "ws-message",
            "ws-close",
            "ws-error",
            "dom-state",
            "history",
            "navigation-api",
            "app-state",
            "request-create",
            "fetch-request-body",
            "resize-observer-create",
            "resize-observer-observe",
            "resize-observer-callback",
            "intersection-observer-create",
            "intersection-observer-observe",
            "intersection-observer-callback",
            "dom-insert",
            "dom-mutation",
            "react-renderer-inject",
            "react-commit",
            "react-commit-trace-error",
            "react-devtools-inject-error",
            "react-devtools-commit-error",
        }
        if event_type in {
            "response-body-read",
            "response-clone",
            "stream-get-reader",
            "stream-read",
        } and isinstance(event.get("source"), dict):
            source_url = str(event["source"].get("url") or "")
            include_event = any(
                marker in source_url
                for marker in (
                    "chatgpt.com/backend-api/f/conversation",
                    "chatgpt.com/backend-api/conversation/",
                    "chatgpt.com/backend-api/celsius/ws/user",
                    "chatgpt.com/backend-api/sentinel/",
                )
            )
        if include_event:
            compact = {
                key: event.get(key)
                for key in (
                    "seq",
                    "t",
                    "type",
                    "url",
                    "method",
                    "status",
                    "ok",
                    "reason",
                    "data",
                    "state",
                    "stateKind",
                    "source",
                    "done",
                    "chunk",
                    "hasBody",
                    "body",
                    "count",
                    "entries",
                    "target",
                    "box",
                    "root",
                    "rootMargin",
                    "parent",
                    "child",
                    "interestingCount",
                    "records",
                    "id",
                    "version",
                    "packageName",
                    "didError",
                    "priorityLevel",
                    "visited",
                    "hostComponents",
                    "hostText",
                    "conversationHints",
                    "messageHints",
                    "turnHints",
                    "markdownHints",
                    "composerHints",
                    "dataTurnProps",
                    "dataMessageProps",
                    "dataRoleProps",
                    "textFiberLenMax",
                    "samples",
                    "name",
                    "message",
                    "op",
                    "optionsKeys",
                    "navigationType",
                    "canIntercept",
                    "hashChange",
                    "userInitiated",
                    "currentEntry",
                    "destination",
                    "from",
                    "entry",
                )
                if key in event
            }
            if isinstance(compact.get("state"), dict):
                if event_type == "app-state":
                    compact["state"] = compact_live_trace_app_state_event(compact["state"])
                else:
                    compact["state"] = compact_live_trace_state_event(compact["state"])
            if isinstance(compact.get("records"), list):
                compact["recordCount"] = len(compact["records"])
                compact["records"] = [compact_live_trace_dom_record(record) for record in compact["records"][:3]]
            if isinstance(compact.get("samples"), list):
                compact["sampleCount"] = len(compact["samples"])
                if event_type in {"dom-mutation", "dom-insert"}:
                    compact["samples"] = compact_live_trace_dom_samples(compact["samples"], limit=5)
                else:
                    compact["samples"] = compact_live_trace_react_samples(compact["samples"], limit=8)
            if event_type in {"fetch-request-body", "request-create"}:
                request_bodies.append(compact)
            interesting_tail.append(compact)
    return {
        "url": trace.get("url"),
        "snapshotErrors": trace.get("snapshotErrors"),
        "state": compact_live_trace_state(trace["state"]) if isinstance(trace.get("state"), dict) else trace.get("state"),
        "eventLoop": trace.get("eventLoop"),
        "domMutations": compact_live_trace_dom_mutations(trace.get("domMutations")),
        "reactCommits": compact_live_trace_react_commits(trace.get("reactCommits")),
        "reactConversationWrappers": compact_live_trace_conversation_wrappers(
            trace.get("reactConversationWrappers")
        ),
        "conversationMaterialization": trace.get("conversationMaterialization"),
        "conversationIdentityTrace": trace.get("conversationIdentityTrace"),
        "reactThreadFiber": compact_live_trace_thread_fiber(trace.get("reactThreadFiber")),
        "threadRendererProbes": compact_live_trace_thread_renderer_probes(
            trace.get("threadRendererProbes")
        ),
        "suspenseBoundaryProbes": compact_live_trace_suspense_boundary_probes(
            trace.get("suspenseBoundaryProbes")
        ),
        "reactionStoreProbes": compact_live_trace_reaction_store_probes(
            trace.get("reactionStoreProbes")
        ),
        "threadStoreHooks": compact_live_trace_thread_store_hooks(trace.get("threadStoreHooks")),
        "navigationApi": trace.get("navigationApi"),
        "idMapTrace": trace.get("idMapTrace"),
        "sourceStats": trace.get("sourceStats"),
        "eventCounts": counts,
        "requestBodies": request_bodies[-20:],
        "interestingTail": interesting_tail[-30:],
    }


def compact_live_trace_router_state(state: Any) -> Any:
    if not isinstance(state, dict):
        return state
    probes = state.get("loaderDataProbes")
    conversation_probe = None
    if isinstance(probes, dict):
        for key, value in probes.items():
            if "conversation" in str(key):
                conversation_probe = {
                    "route": key,
                    "kind": value.get("kind") if isinstance(value, dict) else type(value).__name__,
                    "keys": value.get("keys") if isinstance(value, dict) else None,
                    "thenType": value.get("thenType") if isinstance(value, dict) else None,
                    "mapLikeSize": value.get("mapLikeSize") if isinstance(value, dict) else None,
                }
                break
    return {
        "present": state.get("present"),
        "locationPathname": state.get("locationPathname"),
        "navigationState": state.get("navigationState"),
        "revalidation": state.get("revalidation"),
        "loaderDataKeys": state.get("loaderDataKeys"),
        "conversationProbe": conversation_probe,
        "actionDataKeys": state.get("actionDataKeys"),
        "errorKeys": state.get("errorKeys"),
        "fetcherCount": state.get("fetcherCount"),
        "blockerCount": state.get("blockerCount"),
    }


def compact_live_trace_app_state_event(state: dict[str, Any]) -> dict[str, Any]:
    query_cache = state.get("queryCache")
    query_summary = None
    if isinstance(query_cache, dict):
        query_summary = {
            "present": query_cache.get("present"),
            "queryCount": query_cache.get("queryCount"),
            "statusCounts": query_cache.get("statusCounts"),
            "fetchStatusCounts": query_cache.get("fetchStatusCounts"),
        }
    react_commits = state.get("reactCommits")
    react_summary = None
    if isinstance(react_commits, dict):
        last_commit = react_commits.get("lastCommit")
        last_summary = None
        if isinstance(last_commit, dict):
            last_summary = {
                key: last_commit.get(key)
                for key in (
                    "visited",
                    "hostComponents",
                    "hostText",
                    "conversationHints",
                    "messageHints",
                    "turnHints",
                    "markdownHints",
                    "dataTurnProps",
                    "dataMessageProps",
                    "dataRoleProps",
                )
            }
        react_summary = {
            "commitCount": react_commits.get("commitCount"),
            "commitErrors": react_commits.get("commitErrors"),
            "lastCommitAt": react_commits.get("lastCommitAt"),
            "lastCommit": last_summary,
        }
    return {
        "router": compact_live_trace_router_state(state.get("router")),
        "queryCache": query_summary,
        "serializedAppScripts": state.get("serializedAppScripts"),
        "reactCommits": react_summary,
        "activeElement": state.get("activeElement"),
    }


def compact_live_trace_state_event(state: dict[str, Any]) -> dict[str, Any]:
    selector_census = state.get("selectorCensus")
    selector_counts = {}
    if isinstance(selector_census, dict):
        for name, summary in selector_census.items():
            if isinstance(summary, dict):
                selector_counts[name] = summary.get("count")
    return {
        key: state.get(key)
        for key in (
            "url",
            "readyState",
            "assistantCount",
            "latestAssistantLen",
            "userCount",
            "latestUserLen",
            "stopButtonCount",
            "bodyTextLen",
        )
    } | {
        "selectorCounts": selector_counts,
        "appRuntime": compact_live_trace_app_state_event(state.get("appRuntime"))
        if isinstance(state.get("appRuntime"), dict)
        else None,
        "eventLoop": {
            key: state.get("eventLoop", {}).get(key)
            for key in ("timeoutFired", "intervalFired", "rafFired", "microtaskFired", "heartbeat", "now")
        }
        if isinstance(state.get("eventLoop"), dict)
        else None,
        "domMutations": compact_live_trace_dom_mutations(state.get("domMutations")),
    }


def compact_live_trace_state(state: dict[str, Any]) -> dict[str, Any]:
    selector_census = state.get("selectorCensus")
    selector_counts = {}
    if isinstance(selector_census, dict):
        for name, summary in selector_census.items():
            if isinstance(summary, dict):
                selector_counts[name] = summary.get("count")
    event_loop = state.get("eventLoop")
    compact_event_loop = None
    if isinstance(event_loop, dict):
        compact_event_loop = {
            key: event_loop.get(key)
            for key in (
                "timeoutFired",
                "intervalFired",
                "rafFired",
                "microtaskFired",
                "heartbeat",
                "now",
            )
        }
    app_runtime = state.get("appRuntime")
    compact_app_runtime = compact_live_trace_app_state(app_runtime) if isinstance(app_runtime, dict) else None
    return {
        key: state.get(key)
        for key in (
            "url",
            "readyState",
            "assistantCount",
            "latestAssistantLen",
            "userCount",
            "latestUserLen",
            "stopButtonCount",
            "bodyTextLen",
        )
    } | {
        "selectorCounts": selector_counts,
        "domTree": state.get("domTree"),
        "appRuntime": compact_app_runtime,
        "eventLoop": compact_event_loop,
        "observerApis": state.get("observerApis"),
        "domMutations": compact_live_trace_dom_mutations(state.get("domMutations")),
        "reactCommits": compact_live_trace_react_commits(state.get("reactCommits")),
    }


def compact_live_trace_app_state(state: dict[str, Any]) -> dict[str, Any]:
    return {
        "windowKeys": state.get("windowKeys"),
        "router": state.get("router"),
        "routeModules": state.get("routeModules"),
        "queryCache": state.get("queryCache"),
        "serializedAppScripts": state.get("serializedAppScripts"),
        "observerApis": state.get("observerApis"),
        "reactCommits": compact_live_trace_react_commits(state.get("reactCommits")),
        "activeElement": state.get("activeElement"),
    }


def compact_live_trace_dom_node(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    return {
        key: value.get(key)
        for key in (
            "tag",
            "id",
            "role",
            "testid",
            "dataTurn",
            "classHints",
            "textLen",
            "childCount",
        )
        if key in value
    }


def compact_live_trace_dom_record(record: Any) -> Any:
    if not isinstance(record, dict):
        return record
    added = record.get("added")
    removed = record.get("removed")
    compact = {
        key: record.get(key)
        for key in (
            "type",
            "attributeName",
        )
        if key in record
    }
    compact["target"] = compact_live_trace_dom_node(record.get("target"))
    if isinstance(added, list):
        compact["addedCount"] = len(added)
        compact["added"] = [compact_live_trace_dom_node(node) for node in added[:2]]
    if isinstance(removed, list):
        compact["removedCount"] = len(removed)
        compact["removed"] = [compact_live_trace_dom_node(node) for node in removed[:2]]
    return compact


def compact_live_trace_dom_sample(sample: Any) -> Any:
    if not isinstance(sample, dict):
        return sample
    records = sample.get("records")
    compact = {
        key: sample.get(key)
        for key in (
            "time",
            "kind",
            "count",
            "interestingCount",
        )
        if key in sample
    }
    if isinstance(records, list):
        compact["recordCount"] = len(records)
        compact["records"] = [compact_live_trace_dom_record(record) for record in records[:3]]
    return compact


def compact_live_trace_dom_samples(samples: Any, limit: int = 5) -> Any:
    if not isinstance(samples, list):
        return samples
    return [compact_live_trace_dom_sample(sample) for sample in samples[-limit:]]


def compact_live_trace_dom_mutations(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    samples = value.get("samples")
    return {
        key: value.get(key)
        for key in (
            "mutationObserverRecords",
            "mutationObserverInteresting",
            "appendChildCalls",
            "insertBeforeCalls",
            "replaceChildrenCalls",
            "interestingInserts",
            "interestingRemovals",
            "lastInterestingMutationAt",
            "now",
        )
        if key in value
    } | {
        "sampleCount": len(samples) if isinstance(samples, list) else 0,
        "samples": compact_live_trace_dom_samples(samples, limit=5),
    }


def compact_live_trace_value_shape(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    keys = value.get("keys")
    own_keys = value.get("ownKeys")
    compact = {
        key: value.get(key)
        for key in (
            "kind",
            "tag",
            "constructorName",
            "length",
            "scalar",
            "id",
            "serverId",
            "childKind",
            "textChildLen",
            "collectionKind",
            "size",
            "collectionError",
        )
        if key in value
    }
    fields = value.get("fields")
    if isinstance(fields, dict):
        compact["fields"] = dict(list(fields.items())[:16])
    if isinstance(keys, list):
        compact["keys"] = keys[:12]
    if isinstance(own_keys, list):
        compact["ownKeys"] = own_keys[:12]
    if isinstance(value.get("ctxKeys"), list):
        compact["ctxKeys"] = value["ctxKeys"][:12]
    if isinstance(value.get("configKeys"), list):
        compact["configKeys"] = value["configKeys"][:12]
    zero_arg_result = value.get("zeroArgResult")
    if isinstance(zero_arg_result, dict):
        compact["zeroArgResult"] = compact_live_trace_value_shape(zero_arg_result)
    if "zeroArgError" in value:
        compact["zeroArgError"] = value.get("zeroArgError")
    own_value_shapes = value.get("ownValueShapes")
    if isinstance(own_value_shapes, list):
        compact["ownValueShapes"] = [compact_live_trace_property_shape(item) for item in own_value_shapes[:12]]
    ctx_value_shapes = value.get("ctxOwnValueShapes")
    if isinstance(ctx_value_shapes, list):
        compact["ctxOwnValueShapes"] = [compact_live_trace_property_shape(item) for item in ctx_value_shapes[:12]]
    prototype_methods = value.get("prototypeMethods")
    if isinstance(prototype_methods, list):
        compact["prototypeMethods"] = [
            {
                "key": item.get("key"),
                "kind": item.get("kind"),
                "name": item.get("name"),
                "length": item.get("length"),
                "error": item.get("error"),
            }
            if isinstance(item, dict)
            else item
            for item in prototype_methods[:16]
        ]
    zero_arg_results = value.get("zeroArgFunctionResults")
    if isinstance(zero_arg_results, list):
        compact["zeroArgFunctionResults"] = [
            compact_live_trace_function_result_shape(item) for item in zero_arg_results[:12]
        ]
    ctx_zero_arg_results = value.get("ctxZeroArgFunctionResults")
    if isinstance(ctx_zero_arg_results, list):
        compact["ctxZeroArgFunctionResults"] = [
            compact_live_trace_function_result_shape(item) for item in ctx_zero_arg_results[:12]
        ]
    ctx_store_like = value.get("ctxStoreLike")
    if isinstance(ctx_store_like, list):
        compact["ctxStoreLike"] = [
            compact_live_trace_store_like_summary(item) for item in ctx_store_like[:8]
        ]
    thread_store = value.get("threadStore")
    if isinstance(thread_store, dict):
        compact["threadStore"] = compact_live_trace_thread_store_detail(thread_store)
    query_fields = value.get("queryFields")
    if isinstance(query_fields, dict):
        compact["queryFields"] = {
            key: compact_live_trace_value_shape(item)
            for key, item in list(query_fields.items())[:16]
        }
    for source_key in (
        "dataShape",
        "currentShape",
        "valueShape",
        "errorShape",
        "promiseShape",
    ):
        source_value = value.get(source_key)
        if isinstance(source_value, dict):
            compact[source_key] = compact_live_trace_value_shape(source_value)
    reaction_store = value.get("reactionStore")
    if isinstance(reaction_store, dict):
        compact["reactionStore"] = compact_live_trace_reaction_store_detail(reaction_store)
    conversation_detail = value.get("conversationDetail")
    if isinstance(conversation_detail, dict):
        compact["conversationDetail"] = compact_live_trace_result_detail(conversation_detail)
    selected_values = value.get("selectedValues")
    if isinstance(selected_values, dict):
        compact["selectedValues"] = {
            key: compact_live_trace_value_shape(item)
            for key, item in list(selected_values.items())[:10]
        }
    items = value.get("items")
    if isinstance(items, list):
        compact["itemCount"] = len(items)
        compact["items"] = [compact_live_trace_value_shape(item) for item in items[:3]]
    entries = value.get("entries")
    if isinstance(entries, list):
        compact["entryCount"] = len(entries)
        compact["entries"] = [
            compact_live_trace_collection_entry(item) for item in entries[:6]
        ]
    return compact


def compact_live_trace_thread_renderer_probes(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    compact: dict[str, Any] = {
        "rootPresent": value.get("rootPresent"),
        "visited": value.get("visited"),
        "count": value.get("count"),
    }
    probes = value.get("probes")
    if isinstance(probes, list):
        compact["probes"] = [
            compact_live_trace_thread_renderer_probe(item) for item in probes[:12]
        ]
    return compact


def compact_live_trace_thread_renderer_probe(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    compact = {
        key: value.get(key)
        for key in (
            "depth",
            "name",
            "tag",
            "hints",
            "sourceHint",
        )
        if key in value
    }
    props = value.get("props")
    if isinstance(props, dict):
        compact_props = {
            key: props.get(key)
            for key in (
                "keys",
                "id",
                "role",
                "testid",
                "ariaHidden",
                "inert",
                "dataTurn",
                "dataMessageId",
                "dataMessageAuthorRole",
            )
            if key in props
        }
        selected = props.get("selectedValueShapes")
        if isinstance(selected, dict):
            compact_props["selectedValueShapes"] = {
                key: compact_live_trace_value_shape(item)
                for key, item in list(selected.items())[:8]
            }
        compact["props"] = compact_props
    hooks = value.get("hooks")
    if isinstance(hooks, list):
        compact["hooks"] = [compact_live_trace_hook_state(item) for item in hooks[:16]]
    return compact


def compact_live_trace_suspense_boundary_probes(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    compact: dict[str, Any] = {
        "rootPresent": value.get("rootPresent"),
        "threadFiberPresent": value.get("threadFiberPresent"),
        "visited": value.get("visited"),
        "count": value.get("count"),
    }
    root_lanes = value.get("rootLanes")
    if isinstance(root_lanes, dict):
        compact["rootLanes"] = {
            key: root_lanes.get(key)
            for key in (
                "rootPresent",
                "stateNodePresent",
                "rootFiberLanes",
                "rootFiberChildLanes",
                "pendingLanes",
                "suspendedLanes",
                "pingedLanes",
                "expiredLanes",
                "errorRecoveryDisabledLanes",
                "shellSuspendCounter",
                "entangledLanes",
                "finishedLanes",
            )
            if key in root_lanes
        }
        for key in ("entanglementsShape", "hiddenUpdatesShape"):
            shape = root_lanes.get(key)
            if isinstance(shape, dict):
                compact["rootLanes"][key] = compact_live_trace_value_shape(shape)
    boundaries = value.get("boundaries")
    if isinstance(boundaries, list):
        compact["boundaries"] = [
            compact_live_trace_suspense_boundary(item) for item in boundaries[:16]
        ]
    return compact


def compact_live_trace_suspense_boundary(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    compact = {
        key: value.get(key)
        for key in (
            "source",
            "depth",
            "name",
            "tag",
            "tagLabel",
            "hints",
        )
        if key in value
    }
    path = value.get("path")
    if isinstance(path, list):
        compact["path"] = [
            {
                key: item.get(key)
                for key in ("name", "tag", "tagLabel")
                if isinstance(item, dict) and key in item
            }
            for item in path[-8:]
        ]
    props = value.get("props")
    if isinstance(props, dict):
        compact["props"] = compact_live_trace_suspense_child_props(props)
    internal = value.get("internal")
    if isinstance(internal, dict):
        compact["internal"] = compact_live_trace_suspense_internal(internal)
    return compact


def compact_live_trace_suspense_internal(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    compact = {
        key: value.get(key)
        for key in (
            "mode",
            "flags",
            "subtreeFlags",
            "lanes",
            "childLanes",
        )
        if key in value
    }
    for key in ("memoizedState", "updateQueue", "dependencies"):
        item = value.get(key)
        if isinstance(item, dict):
            compact[key] = compact_live_trace_fiber_internal_field(item)
    children = value.get("children")
    if isinstance(children, list):
        compact["children"] = [
            compact_live_trace_suspense_child(item) for item in children[:8]
        ]
    element_tree = value.get("elementTree")
    if isinstance(element_tree, dict):
        compact["elementTree"] = compact_live_trace_react_element_tree(element_tree)
    alternate = value.get("alternate")
    if isinstance(alternate, dict):
        compact["alternate"] = compact_live_trace_suspense_alternate(alternate)
    return compact


def compact_live_trace_suspense_alternate(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    compact = {
        key: value.get(key)
        for key in (
            "present",
            "name",
            "tag",
            "tagLabel",
            "mode",
            "flags",
            "subtreeFlags",
            "lanes",
            "childLanes",
        )
        if key in value
    }
    for key in ("memoizedState", "updateQueue", "dependencies"):
        item = value.get(key)
        if isinstance(item, dict):
            compact[key] = compact_live_trace_fiber_internal_field(item)
    children = value.get("children")
    if isinstance(children, list):
        compact["children"] = [
            compact_live_trace_suspense_child(item) for item in children[:8]
        ]
    element_tree = value.get("elementTree")
    if isinstance(element_tree, dict):
        compact["elementTree"] = compact_live_trace_react_element_tree(element_tree)
    return compact


def compact_live_trace_react_element_tree(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    kind = value.get("kind")
    if kind == "array":
        items = value.get("items")
        return {
            "kind": "array",
            "length": value.get("length"),
            "items": [
                compact_live_trace_react_element_tree(item) for item in items[:6]
            ]
            if isinstance(items, list)
            else [],
        }
    if kind == "react-element":
        compact: dict[str, Any] = {
            "kind": "react-element",
            "key": value.get("key"),
        }
        type_summary = value.get("type")
        if isinstance(type_summary, dict):
            compact["type"] = compact_live_trace_react_element_type(type_summary)
        prop_keys = value.get("propKeys")
        if isinstance(prop_keys, list):
            compact["propKeys"] = prop_keys[:12]
        selected_props = value.get("selectedProps")
        if isinstance(selected_props, dict):
            compact["selectedProps"] = {
                key: compact_live_trace_reference_shape(item)
                for key, item in list(selected_props.items())[:8]
            }
        children = value.get("children")
        if isinstance(children, dict):
            compact["children"] = compact_live_trace_react_element_tree(children)
        fallback = value.get("fallback")
        if isinstance(fallback, dict):
            compact["fallback"] = compact_live_trace_react_element_tree(fallback)
        return compact
    return compact_live_trace_value_shape(value)


def compact_live_trace_react_element_type(value: Any, depth: int = 0) -> Any:
    if not isinstance(value, dict):
        return value
    if depth >= 3:
        return {"kind": "nested"}
    compact = {
        key: value.get(key)
        for key in (
            "kind",
            "name",
            "tag",
            "keys",
            "ownKeys",
            "hasLazyInit",
            "sourceHint",
            "error",
        )
        if key in value
    }
    lazy_payload = value.get("lazyPayload")
    if isinstance(lazy_payload, dict):
        compact_payload: dict[str, Any] = {
            "status": lazy_payload.get("status"),
        }
        shape = lazy_payload.get("shape")
        if isinstance(shape, dict):
            compact_payload["shape"] = compact_live_trace_value_shape(shape)
        own_value_shapes = lazy_payload.get("ownValueShapes")
        if isinstance(own_value_shapes, list):
            compact_payload["ownValueShapes"] = [
                compact_live_trace_property_shape(item) for item in own_value_shapes[:8]
            ]
        result = lazy_payload.get("result")
        if isinstance(result, dict):
            compact_payload["result"] = compact_live_trace_value_shape(result)
        result_own_value_shapes = lazy_payload.get("resultOwnValueShapes")
        if isinstance(result_own_value_shapes, list):
            compact_payload["resultOwnValueShapes"] = [
                compact_live_trace_property_shape(item)
                for item in result_own_value_shapes[:8]
            ]
        compact["lazyPayload"] = compact_payload
    render = value.get("render")
    if isinstance(render, dict):
        compact["render"] = compact_live_trace_react_element_type(render, depth + 1)
    inner_type = value.get("innerType")
    if isinstance(inner_type, dict):
        compact["innerType"] = compact_live_trace_react_element_type(
            inner_type, depth + 1
        )
    return compact


def compact_live_trace_fiber_internal_field(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    compact: dict[str, Any] = {}
    for key in ("present", "error", "ownValueShapesError", "selectedValuesError"):
        if key in value:
            compact[key] = value.get(key)
    shape = value.get("shape")
    if isinstance(shape, dict):
        compact["shape"] = compact_live_trace_value_shape(shape)
    own_value_shapes = value.get("ownValueShapes")
    if isinstance(own_value_shapes, list):
        compact["ownValueShapes"] = [
            compact_live_trace_property_shape(item) for item in own_value_shapes[:10]
        ]
    selected_values = value.get("selectedValues")
    if isinstance(selected_values, dict):
        compact["selectedValues"] = {
            key: compact_live_trace_value_shape(item)
            for key, item in list(selected_values.items())[:10]
        }
    return compact


def compact_live_trace_suspense_child(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    compact = {
        key: value.get(key)
        for key in (
            "index",
            "name",
            "tag",
            "tagLabel",
            "hints",
            "lanes",
            "childLanes",
            "flags",
            "subtreeFlags",
        )
        if key in value
    }
    props = value.get("props")
    if isinstance(props, dict):
        compact["props"] = compact_live_trace_suspense_child_props(props)
    memoized_state = value.get("memoizedState")
    if isinstance(memoized_state, dict):
        compact["memoizedState"] = compact_live_trace_fiber_internal_field(memoized_state)
    state_node = value.get("stateNode")
    if isinstance(state_node, dict):
        compact["stateNode"] = state_node
    return compact


def compact_live_trace_suspense_child_props(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    compact = {
        key: value.get(key)
        for key in (
            "keys",
            "id",
            "role",
            "testid",
            "ariaHidden",
            "inert",
            "dataTurn",
            "dataMessageId",
            "dataMessageAuthorRole",
        )
        if key in value
    }
    selected = value.get("selectedValueShapes")
    if isinstance(selected, dict):
        compact["selectedValueShapes"] = {
            key: compact_live_trace_reference_shape(item)
            for key, item in list(selected.items())[:8]
        }
    return compact


def compact_live_trace_thread_store_detail(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    compact: dict[str, Any] = {}
    mapping = value.get("mapping")
    if isinstance(mapping, dict):
        compact["mapping"] = {
            "count": mapping.get("count"),
            "entries": mapping.get("entries")[:8] if isinstance(mapping.get("entries"), list) else [],
        }
    for key in ("mappingError", "threadCount", "threadKeys", "threadsError"):
        if key in value:
            compact[key] = value.get(key)
    return compact


def compact_live_trace_reference_shape(value: Any) -> Any:
    """Very shallow object shape for React props embedded in timeout errors."""
    if not isinstance(value, dict):
        return value
    keys = value.get("keys")
    own_keys = value.get("ownKeys")
    compact = {
        key: value.get(key)
        for key in (
            "kind",
            "tag",
            "constructorName",
            "length",
            "scalar",
            "id",
            "serverId",
            "childKind",
            "textChildLen",
            "collectionKind",
            "size",
        )
        if key in value
    }
    if isinstance(keys, list):
        compact["keys"] = keys[:8]
    if isinstance(own_keys, list):
        compact["ownKeys"] = own_keys[:8]
    if isinstance(value.get("ctxKeys"), list):
        compact["ctxKeys"] = value["ctxKeys"][:8]
    if isinstance(value.get("configKeys"), list):
        compact["configKeys"] = value["configKeys"][:8]
    zero_arg_result = value.get("zeroArgResult")
    if isinstance(zero_arg_result, dict):
        compact["zeroArgResult"] = compact_live_trace_value_shape(zero_arg_result)
    if "zeroArgError" in value:
        compact["zeroArgError"] = value.get("zeroArgError")
    items = value.get("items")
    if isinstance(items, list):
        compact["itemCount"] = len(items)
    entries = value.get("entries")
    if isinstance(entries, list):
        compact["entryCount"] = len(entries)
    if "error" in value:
        compact["error"] = value.get("error")
    return compact


def compact_live_trace_collection_entry(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    compact: dict[str, Any] = {}
    if "index" in value:
        compact["index"] = value.get("index")
    key_shape = value.get("key")
    if isinstance(key_shape, dict):
        compact["keyShape"] = compact_live_trace_value_shape(key_shape)
    elif key_shape is not None:
        compact["key"] = key_shape
    item_shape = value.get("value")
    if isinstance(item_shape, dict):
        compact["itemShape"] = compact_live_trace_value_shape(item_shape)
    elif item_shape is not None:
        compact["itemShape"] = item_shape
    own_shapes = value.get("valueOwnValueShapes")
    if isinstance(own_shapes, list):
        compact["itemOwnValueShapes"] = [
            compact_live_trace_property_shape(item) for item in own_shapes[:6]
        ]
    return compact


def compact_live_trace_property_shape(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    compact: dict[str, Any] = {"key": value.get("key")}
    if "missingDescriptor" in value:
        compact["missingDescriptor"] = value.get("missingDescriptor")
    if "error" in value:
        compact["error"] = value.get("error")
    accessor = value.get("accessor")
    if isinstance(accessor, dict):
        compact["accessor"] = {
            "get": accessor.get("get"),
            "set": accessor.get("set"),
        }
    prop_shape = value.get("shape")
    if isinstance(prop_shape, dict):
        compact["shape"] = compact_live_trace_value_shape(prop_shape)
    elif prop_shape is not None:
        compact["shape"] = prop_shape
    return compact


def compact_live_trace_function_result_shape(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    compact: dict[str, Any] = {
        "key": value.get("key"),
        "index": value.get("index"),
        "name": value.get("name"),
    }
    if "error" in value:
        compact["error"] = value.get("error")
    result_shape = value.get("resultShape")
    if isinstance(result_shape, dict):
        compact["resultShape"] = compact_live_trace_value_shape(result_shape)
    elif result_shape is not None:
        compact["resultShape"] = result_shape
    result_detail = value.get("resultDetail")
    if isinstance(result_detail, dict):
        compact["resultDetail"] = compact_live_trace_result_detail(result_detail)
    return compact


def compact_live_trace_store_like_summary(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    compact: dict[str, Any] = {"key": value.get("key")}
    for key in ("listenerCount", "error", "stateError"):
        if key in value:
            compact[key] = value.get(key)
    shape = value.get("shape")
    if isinstance(shape, dict):
        compact["shape"] = compact_live_trace_value_shape(shape)
    state_shape = value.get("stateShape")
    if isinstance(state_shape, dict):
        compact["stateShape"] = compact_live_trace_value_shape(state_shape)
    state_own_value_shapes = value.get("stateOwnValueShapes")
    if isinstance(state_own_value_shapes, list):
        compact["stateOwnValueShapes"] = [
            compact_live_trace_property_shape(item) for item in state_own_value_shapes[:12]
        ]
    selected_state_values = value.get("selectedStateValues")
    if isinstance(selected_state_values, dict):
        compact["selectedStateValues"] = {
            key: compact_live_trace_value_shape(item)
            for key, item in list(selected_state_values.items())[:12]
        }
    return compact


def compact_live_trace_result_detail(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    compact: dict[str, Any] = {}
    keys = value.get("keys")
    if isinstance(keys, list):
        compact["keys"] = keys[:16]
    own_keys = value.get("ownKeys")
    if isinstance(own_keys, list):
        compact["ownKeys"] = own_keys[:16]
    tree_shape = value.get("treeShape")
    if isinstance(tree_shape, dict):
        compact["treeShape"] = compact_live_trace_value_shape(tree_shape)
    if "treeCurrentLeafId" in value:
        compact["treeCurrentLeafId"] = value.get("treeCurrentLeafId")
    tree_nodes = value.get("treeNodes")
    if isinstance(tree_nodes, dict):
        compact["treeNodes"] = compact_live_trace_value_shape(tree_nodes)
    tree_display_items = value.get("treeDisplayItems")
    if isinstance(tree_display_items, dict):
        compact["treeDisplayItems"] = compact_live_trace_value_shape(tree_display_items)
    tree_display_turns = value.get("treeDisplayTurns")
    if isinstance(tree_display_turns, dict):
        compact["treeDisplayTurns"] = compact_live_trace_value_shape(tree_display_turns)
    tree_own_value_shapes = value.get("treeOwnValueShapes")
    if isinstance(tree_own_value_shapes, list):
        compact["treeOwnValueShapes"] = [
            compact_live_trace_property_shape(item) for item in tree_own_value_shapes[:12]
        ]
    tree_prototype_methods = value.get("treePrototypeMethods")
    if isinstance(tree_prototype_methods, list):
        compact["treePrototypeMethods"] = [
            {
                "key": item.get("key"),
                "kind": item.get("kind"),
                "name": item.get("name"),
                "length": item.get("length"),
                "error": item.get("error"),
            }
            if isinstance(item, dict)
            else item
            for item in tree_prototype_methods[:16]
        ]
    tree_zero_arg_results = value.get("treeZeroArgFunctionResults")
    if isinstance(tree_zero_arg_results, list):
        compact["treeZeroArgFunctionResults"] = [
            compact_live_trace_function_result_shape(item) for item in tree_zero_arg_results[:8]
        ]
    data_shape = value.get("dataShape")
    if isinstance(data_shape, dict):
        compact["dataShape"] = compact_live_trace_value_shape(data_shape)
    data_own_value_shapes = value.get("dataOwnValueShapes")
    if isinstance(data_own_value_shapes, list):
        compact["dataOwnValueShapes"] = [
            compact_live_trace_property_shape(item) for item in data_own_value_shapes[:8]
        ]
    for key in (
        "treeError",
        "treeCurrentLeafIdError",
        "treeNodesError",
        "treeDisplayItemsError",
        "treeDisplayTurnsError",
        "dataError",
    ):
        if key in value:
            compact[key] = value.get(key)
    return compact


def compact_live_trace_react_sample(sample: Any) -> Any:
    if not isinstance(sample, dict):
        return sample
    props = sample.get("props")
    compact_props: dict[str, Any] | None = None
    if isinstance(props, dict):
        compact_props = {
            key: props.get(key)
            for key in (
                "kind",
                "keys",
                "id",
                "role",
                "testid",
                "ariaHidden",
                "inert",
                "dataTurn",
                "dataMessageId",
                "dataMessageAuthorRole",
                "classHints",
                "childKind",
                "textChildLen",
                "hasDangerousHtml",
            )
            if key in props
        }
        selected_shapes = props.get("selectedValueShapes")
        if isinstance(selected_shapes, dict):
            compact_props["selectedValueShapes"] = {
                key: compact_live_trace_reference_shape(value)
                for key, value in list(selected_shapes.items())[:6]
            }
    return {
        key: sample.get(key)
        for key in (
            "name",
            "tag",
            "hints",
        )
        if key in sample
    } | {
        "props": compact_props,
        "hasStateNode": sample.get("stateNode") is not None,
    }


def compact_live_trace_conversation_wrapper(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    compact: dict[str, Any] = {
        "name": value.get("name"),
        "tag": value.get("tag"),
    }
    hints = value.get("hints")
    if isinstance(hints, list):
        compact["hints"] = hints[:8]
    conversation = value.get("conversation")
    if isinstance(conversation, dict):
        compact["conversation"] = compact_live_trace_value_shape(conversation)
    elif conversation is not None:
        compact["conversation"] = conversation
    subtree = value.get("subtree")
    if isinstance(subtree, dict):
        compact["subtree"] = compact_live_trace_conversation_subtree(subtree)
    for key in ("error", "message"):
        if key in value:
            compact[key] = value.get(key)
    return compact


def compact_live_trace_conversation_subtree(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    compact: dict[str, Any] = {
        "visited": value.get("visited"),
        "count": value.get("count"),
    }
    nodes = value.get("nodes")
    if isinstance(nodes, list):
        compact["nodes"] = [
            compact_live_trace_conversation_subtree_node(item) for item in nodes[:40]
        ]
    return compact


def compact_live_trace_conversation_subtree_node(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    compact = {
        key: value.get(key)
        for key in (
            "depth",
            "name",
            "tag",
            "hints",
            "sourceHint",
        )
        if key in value
    }
    props = value.get("props")
    if isinstance(props, dict):
        compact_props = {
            key: props.get(key)
            for key in (
                "keys",
                "id",
                "role",
                "testid",
                "ariaHidden",
                "inert",
                "dataTurn",
                "dataMessageId",
                "dataMessageAuthorRole",
            )
            if key in props
        }
        selected = props.get("selectedValueShapes")
        if isinstance(selected, dict):
            compact_props["selectedValueShapes"] = {
                key: compact_live_trace_reference_shape(item)
                for key, item in list(selected.items())[:8]
            }
        compact["props"] = compact_props
    hooks = value.get("hooks")
    if isinstance(hooks, list):
        compact["hooks"] = [compact_live_trace_hook_state(item) for item in hooks[:8]]
    state_node = value.get("stateNode")
    if isinstance(state_node, dict):
        compact["stateNode"] = state_node
    return compact


def compact_live_trace_hook_state(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    compact: dict[str, Any] = {}
    for key in (
        "index",
        "hookKeys",
        "memoizedStateError",
        "baseStateError",
        "queueError",
        "queueSnapshotError",
    ):
        if key in value:
            compact[key] = value.get(key)
    for source_key in (
        "memoizedState",
        "baseState",
        "queueValue",
        "lastRenderedState",
        "queueSnapshot",
    ):
        source_value = value.get(source_key)
        if isinstance(source_value, dict):
            compact[source_key] = compact_live_trace_value_shape(source_value)
    queue_shape = value.get("queueShape")
    if isinstance(queue_shape, dict):
        compact["queueShape"] = compact_live_trace_value_shape(queue_shape)
    return compact


def compact_live_trace_thread_store_hooks(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    compact: dict[str, Any] = {
        "rootPresent": value.get("rootPresent"),
        "visited": value.get("visited"),
        "count": value.get("count"),
    }
    snapshots = value.get("snapshots")
    if isinstance(snapshots, list):
        compact["snapshots"] = [
            compact_live_trace_thread_store_hook_snapshot(item) for item in snapshots[:8]
        ]
    return compact


def compact_live_trace_thread_store_hook_snapshot(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    compact = {
        key: value.get(key)
        for key in (
            "depth",
            "name",
            "tag",
            "hookIndex",
            "source",
        )
        if key in value
    }
    detail = value.get("detail")
    if isinstance(detail, dict):
        compact["detail"] = compact_live_trace_value_shape(detail)
    return compact


def compact_live_trace_reaction_store_probes(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    compact: dict[str, Any] = {
        "rootPresent": value.get("rootPresent"),
        "visited": value.get("visited"),
        "count": value.get("count"),
    }
    probes = value.get("probes")
    if isinstance(probes, list):
        compact["probes"] = [
            compact_live_trace_reaction_store_probe(item) for item in probes[:64]
        ]
    return compact


def compact_live_trace_reaction_store_probe(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    compact = {
        key: value.get(key)
        for key in (
            "source",
            "depth",
            "name",
            "tag",
            "tagLabel",
            "hints",
            "hookIndex",
            "hookSource",
            "selectedThreadProps",
        )
        if key in value
    }
    path = value.get("path")
    if isinstance(path, list):
        compact["path"] = [
            {
                key: item.get(key)
                for key in ("name", "tag", "tagLabel")
                if isinstance(item, dict) and key in item
            }
            for item in path[:10]
        ]
    detail = value.get("detail")
    if isinstance(detail, dict):
        compact["detail"] = compact_live_trace_reaction_store_detail(detail)
    return compact


def compact_live_trace_reaction_store_detail(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    compact: dict[str, Any] = {
        key: value.get(key)
        for key in (
            "kind",
            "object",
            "hasSubscribe",
            "hasGetSnapshot",
            "hasEvaluate",
            "hasOnStoreChange",
        )
        if key in value
    }
    if isinstance(value.get("ownKeys"), list):
        compact["ownKeys"] = value["ownKeys"][:12]
    for source_key in ("stateVersion", "name", "reactionShape"):
        source_value = value.get(source_key)
        if isinstance(source_value, dict):
            compact[source_key] = compact_live_trace_value_shape(source_value)
    for source_key in ("lastValue", "snapshot"):
        source_value = value.get(source_key)
        if isinstance(source_value, dict):
            compact[source_key] = compact_live_trace_value_shape(source_value)
    for error_key in (
        "stateVersionError",
        "nameError",
        "lastValueError",
        "snapshotError",
        "reactionError",
    ):
        if error_key in value:
            compact[error_key] = value.get(error_key)
    return compact


def compact_live_trace_conversation_wrappers(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    compact: dict[str, Any] = {
        "rootPresent": value.get("rootPresent"),
        "visited": value.get("visited"),
        "count": value.get("count"),
    }
    snapshots = value.get("snapshots")
    if isinstance(snapshots, list):
        compact["snapshots"] = [
            compact_live_trace_conversation_wrapper(item) for item in snapshots[:4]
        ]
    return compact


def compact_live_trace_thread_fiber(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    compact: dict[str, Any] = {
        "fiberPresent": value.get("fiberPresent"),
    }
    thread = value.get("thread")
    if isinstance(thread, dict):
        compact["thread"] = thread
    ancestors = value.get("ancestors")
    if isinstance(ancestors, list):
        compact["ancestors"] = [
            compact_live_trace_conversation_subtree_node(item) for item in ancestors[:36]
        ]
    subtree = value.get("subtree")
    if isinstance(subtree, dict):
        compact["subtree"] = compact_live_trace_conversation_subtree(subtree)
    return compact


def compact_live_trace_react_samples(samples: Any, limit: int = 8) -> Any:
    if not isinstance(samples, list):
        return samples
    return [compact_live_trace_react_sample(sample) for sample in samples[:limit]]


def compact_live_trace_react_commits(value: Any) -> Any:
    if not isinstance(value, dict):
        return value
    last_commit = value.get("lastCommit")
    compact_last_commit = None
    if isinstance(last_commit, dict):
        compact_last_commit = {
            key: last_commit.get(key)
            for key in (
                "id",
                "didError",
                "priorityLevel",
                "visited",
                "hostComponents",
                "hostText",
                "conversationHints",
                "messageHints",
                "turnHints",
                "markdownHints",
                "composerHints",
                "dataTurnProps",
                "dataMessageProps",
                "dataRoleProps",
                "textFiberLenMax",
            )
        }
        samples = last_commit.get("samples")
        if isinstance(samples, list):
            compact_last_commit["sampleCount"] = len(samples)
            compact_last_commit["samples"] = compact_live_trace_react_samples(samples, limit=8)
    samples = value.get("samples")
    return {
        key: value.get(key)
        for key in (
            "hookInstalled",
            "hookPreexisting",
            "rendererCount",
            "commitCount",
            "commitErrors",
            "lastCommitAt",
            "lastRendererId",
            "now",
        )
    } | {
        "lastCommit": compact_last_commit,
        "sampleCount": len(samples) if isinstance(samples, list) else 0,
        "samples": compact_live_trace_react_samples(samples[-8:] if isinstance(samples, list) else samples),
    }


def suppress_known_navigation_context_loss(loop: asyncio.AbstractEventLoop) -> None:
    previous_handler = loop.get_exception_handler()

    def handler(current_loop: asyncio.AbstractEventLoop, context: dict[str, Any]) -> None:
        exception = context.get("exception")
        if isinstance(exception, BaseException) and looks_like_navigation_context_loss(exception):
            return
        if previous_handler is not None:
            previous_handler(current_loop, context)
        else:
            current_loop.default_exception_handler(context)

    loop.set_exception_handler(handler)


class PlaywrightChatGPTSession:
    def __init__(self, args: Namespace, reporter: Reporter = print) -> None:
        self.args = args
        self.reporter = reporter
        self.serve: MoliServe | None = None
        self.playwright: Any = None
        self.browser: Any = None
        self.context: Any = None
        self.page: Any = None
        self._close_context = False
        self.console_tail: list[str] = []
        self.page_error_tail: list[str] = []
        self.request_failed_tail: list[str] = []
        self.request_tail: list[str] = []
        self.response_tail: list[str] = []
        self.conversation_network_tail: list[str] = []
        self._auth_code_cache: str | None = None

    def report(self, message: str) -> None:
        if self.reporter is not None:
            self.reporter(message)

    def live_trace_enabled(self) -> bool:
        return bool(
            getattr(self.args, "live_trace", False)
            or getattr(self.args, "live_trace_output", "")
        )

    async def start(self) -> None:
        suppress_known_navigation_context_loss(asyncio.get_running_loop())
        try:
            from playwright.async_api import async_playwright
        except ImportError as error:
            raise DemoError(
                "missing dependency: playwright; run with "
                "`uv run --with websockets --with playwright python chatgpt_playwright_tui.py`"
            ) from error

        self.playwright = await async_playwright().start()
        backend = str(getattr(self.args, "backend", "moli") or "moli")
        if backend == "chromium":
            await self.start_chromium_backend()
        elif backend == "moli":
            await self.start_moli_backend()
        else:
            raise DemoError(f"unsupported Playwright backend: {backend!r}")

    async def start_moli_backend(self) -> None:
        if self.playwright is None:
            raise DemoError("Playwright is not initialized")
        self.report("starting moli serve")
        self.args.quiet = True
        self.serve = start_moli(self.args)
        self.report(f"connected CDP at {self.serve.endpoint}")
        version = await asyncio.to_thread(read_json_url_no_proxy, self.serve.endpoint.rstrip("/") + "/json/version")
        websocket_url = version.get("webSocketDebuggerUrl")
        if not isinstance(websocket_url, str) or not websocket_url:
            raise DemoError(f"missing webSocketDebuggerUrl in CDP version payload: {version!r}")
        self.browser = await self.playwright.chromium.connect_over_cdp(websocket_url)
        self.context = self.browser.contexts[0] if self.browser.contexts else await self.browser.new_context()
        await self.install_live_trace_init_script()
        await self.finish_page_setup()

    async def start_chromium_backend(self) -> None:
        if self.playwright is None:
            raise DemoError("Playwright is not initialized")
        launch_options: dict[str, Any] = {
            "headless": not bool(getattr(self.args, "headful", False)),
        }
        chromium_bin = str(getattr(self.args, "chromium_bin", "") or "")
        if chromium_bin:
            launch_options["executable_path"] = chromium_bin
        proxy = str(getattr(self.args, "http_proxy", "") or "")
        if proxy:
            launch_options["proxy"] = {"server": proxy}
            no_proxy = str(getattr(self.args, "http_no_proxy", "") or "")
            if no_proxy:
                launch_options["proxy"]["bypass"] = no_proxy
        context_options: dict[str, Any] = {}
        user_agent = str(getattr(self.args, "user_agent", "") or "")
        if user_agent:
            context_options["user_agent"] = user_agent
        profile_dir = str(getattr(self.args, "profile_dir", "") or "")
        if profile_dir:
            self.report("starting chromium persistent context")
            self.context = await self.playwright.chromium.launch_persistent_context(
                profile_dir,
                **launch_options,
                **context_options,
            )
        else:
            self.report("starting chromium")
            self.browser = await self.playwright.chromium.launch(**launch_options)
            self.context = await self.browser.new_context(**context_options)
        self._close_context = True
        await self.install_live_trace_init_script()
        await self.finish_page_setup()

    async def finish_page_setup(self) -> None:
        if self.context is None:
            raise DemoError("Playwright context is not initialized")
        self.page = self.context.pages[0] if self.context.pages else await self.context.new_page()
        self.page.set_default_timeout(10_000)
        self.page.on("console", self._record_console)
        self.page.on("pageerror", self._record_page_error)
        self.page.on("request", self._record_request)
        self.page.on("response", self._record_response)
        self.page.on("requestfailed", self._record_request_failed)
        self.page.on(
            "framenavigated",
            lambda frame: self.report(f"frame navigated: {redact_diagnostic_text(frame.url)}")
            if frame == self.page.main_frame
            else None,
        )

    async def install_live_trace_init_script(self) -> None:
        if not self.live_trace_enabled() or self.context is None:
            return
        self.report("install live trace")
        await self.context.add_init_script(CHATGPT_LIVE_TRACE_JS)

    def _append_tail(self, target: list[str], text: str, *, limit: int = 20) -> None:
        target.append(text)
        if len(target) > limit:
            del target[: len(target) - limit]

    def _record_console(self, message: Any) -> None:
        location = getattr(message, "location", {}) or {}
        location_text = ""
        if isinstance(location, dict) and location.get("url"):
            location_text = (
                f" @{redact_diagnostic_text(str(location.get('url') or ''))}:"
                f"{location.get('lineNumber', '')}:{location.get('columnNumber', '')}"
            )
        text = f"{getattr(message, 'type', '')}: {getattr(message, 'text', '')}{location_text}"
        self._append_tail(self.console_tail, text[:1000])
        args = list(getattr(message, "args", []) or [])
        if args and getattr(message, "type", "") in {"error", "warning"}:
            try:
                loop = asyncio.get_running_loop()
                loop.create_task(self._record_console_arg_details(args, location_text))
            except RuntimeError:
                pass

    async def _record_console_arg_details(self, args: list[Any], location_text: str) -> None:
        details: list[str] = []
        for handle in args[:4]:
            try:
                value = await handle.evaluate(
                    """value => {
                      if (value instanceof Error) {
                        return {
                          kind: 'Error',
                          name: value.name,
                          message: value.message,
                          stack: value.stack,
                          cause: value.cause && String(value.cause)
                        };
                      }
                      if (value && typeof value === 'object') {
                        return {
                          kind: Object.prototype.toString.call(value),
                          name: value.name,
                          message: value.message,
                          code: value.code,
                          stack: value.stack,
                          text: String(value)
                        };
                      }
                      return {kind: typeof value, text: String(value)};
                    }"""
                )
            except Exception as error:
                details.append(f"<arg-error {type(error).__name__}: {error}>")
                continue
            details.append(redact_diagnostic_text(repr(value))[:700])
        if details:
            self._append_tail(
                self.console_tail,
                f"console-args{location_text}: {' | '.join(details)}"[:2000],
            )

    def _record_page_error(self, error: BaseException) -> None:
        self._append_tail(self.page_error_tail, (str(error) or repr(error))[:1000])

    def _record_request_failed(self, request: Any) -> None:
        failure = request.failure
        detail = failure if isinstance(failure, str) else str(failure)
        self._append_tail(
            self.request_failed_tail,
            redact_diagnostic_text(f"{request.method} {request.url} {detail}"[:1000]),
        )
        if is_conversation_diagnostic_url(request.url):
            self._append_tail(
                self.conversation_network_tail,
                redact_diagnostic_text(f"FAILED {request.method} {request.resource_type} {request.url} {detail}"[:1000]),
                limit=300,
            )

    def _record_request(self, request: Any) -> None:
        if not is_diagnostic_url(request.url):
            return
        message = redact_diagnostic_text(f"{request.method} {request.resource_type} {request.url}"[:1000])
        self._append_tail(self.request_tail, message, limit=200)
        if is_conversation_diagnostic_url(request.url):
            self._append_tail(self.conversation_network_tail, f"REQ {message}", limit=300)
            if "chatgpt.com/backend-api/f/conversation" in request.url:
                try:
                    post_data = getattr(request, "post_data", None)
                    if callable(post_data):
                        post_data = post_data()
                except Exception:
                    post_data = None
                shape = chat_request_post_data_shape(post_data if isinstance(post_data, str) else None)
                if shape is not None:
                    self._append_tail(
                        self.conversation_network_tail,
                        f"REQ_BODY_SHAPE {redact_snapshot(shape)!r}",
                        limit=300,
                    )

    def _record_response(self, response: Any) -> None:
        url = response.url
        if not is_diagnostic_url(url):
            return
        headers = getattr(response, "headers", {}) or {}
        details: list[str] = []
        set_cookie = headers.get("set-cookie") or headers.get("Set-Cookie")
        if isinstance(set_cookie, str) and set_cookie:
            names = set_cookie_names_from_header(set_cookie)
            details.append(f"set-cookie={','.join(names) if names else '<present>'}")
        location = headers.get("location") or headers.get("Location")
        if isinstance(location, str) and location:
            details.append(f"location={redact_diagnostic_text(location)}")
        suffix = f" [{' '.join(details)}]" if details else ""
        message = redact_diagnostic_text(f"{response.status} {url}{suffix}"[:1000])
        self._append_tail(self.response_tail, message, limit=200)
        if is_conversation_diagnostic_url(url):
            self._append_tail(self.conversation_network_tail, f"RES {message}", limit=300)

    def diagnostic_tail(self) -> dict[str, list[str]]:
        return {
            "console": self.console_tail[-10:],
            "page_errors": self.page_error_tail[-10:],
            "requests": self.request_tail[-60:],
            "responses": self.response_tail[-60:],
            "conversation_network": self.conversation_network_tail[-120:],
            "request_failed": self.request_failed_tail[-10:],
        }

    async def live_trace_snapshot_best_effort(self, *, timeout: float = 5.0) -> dict[str, Any] | None:
        if not self.live_trace_enabled() or self.page is None:
            return None
        try:
            trace = await asyncio.wait_for(
                self.page.evaluate(
                    """() => {
                      if (typeof window.__lmChatGPTLiveTraceSnapshot !== 'function') return null;
                      return window.__lmChatGPTLiveTraceSnapshot();
                    }"""
                ),
                timeout=timeout,
            )
        except Exception as error:  # noqa: BLE001 - diagnostics should not hide the primary failure.
            return {"error": type(error).__name__, "message": str(error)}
        return trace if isinstance(trace, dict) else None

    async def write_live_trace_summary(
        self,
        reason: str,
        summary: dict[str, Any] | None = None,
    ) -> None:
        output = str(getattr(self.args, "live_trace_output", "") or "")
        if not output:
            return
        try:
            if summary is None:
                summary = summarize_live_trace(await self.live_trace_snapshot_best_effort(timeout=5))
            payload = redact_snapshot(
                {
                    "reason": reason,
                    "timestamp": time.time(),
                    "url": self.page.url if self.page is not None else "",
                    "trace": summary,
                }
            )
            with open(output, "a", encoding="utf-8") as handle:
                handle.write(json.dumps(payload, ensure_ascii=False, sort_keys=True))
                handle.write("\n")
            self.report(f"wrote live trace summary: {output}")
        except Exception as error:  # noqa: BLE001 - diagnostics should not hide the primary result.
            self.report(f"failed to write live trace summary: {type(error).__name__}: {error}")

    async def close(self) -> None:
        if self.context is not None and self._close_context:
            try:
                await self.context.close()
            except Exception:
                pass
            self.context = None
        if self.browser is not None:
            try:
                await self.browser.close()
            except Exception:
                pass
            self.browser = None
        if self.playwright is not None:
            try:
                await self.playwright.stop()
            except Exception:
                pass
            self.playwright = None
        if self.serve is not None:
            stop_moli(self.serve)
            self.serve = None

    async def install_helpers(self) -> None:
        if self.page is None:
            raise DemoError("Playwright page is not initialized")
        if await self.page.evaluate("!!window.__lmChatGPTDemo"):
            return
        await self.page.evaluate(CHATGPT_HELPER_JS)

    async def wait_for_document_settle(self, *, timeout: float = 5.0) -> None:
        if self.page is None:
            raise DemoError("Playwright page is not initialized")
        try:
            await self.page.wait_for_load_state("domcontentloaded", timeout=int(timeout * 1000))
        except Exception:
            pass
        await asyncio.sleep(0.25)

    async def helper(self, name: str, *args: Any, timeout: float = 10.0) -> Any:
        if self.page is None:
            raise DemoError("Playwright page is not initialized")
        read_only_helper = name in {"loginState", "conversationState", "latestAssistantText", "snapshot"}
        attempts = 2 if read_only_helper else 1
        async def invoke() -> Any:
            await self.install_helpers()
            return await self.page.evaluate(
                """([name, args]) => window.__lmChatGPTDemo[name](...args)""",
                [name, list(args)],
            )

        for attempt in range(attempts):
            try:
                return await asyncio.wait_for(invoke(), timeout=timeout)
            except TimeoutError as error:
                if read_only_helper and attempt + 1 < attempts:
                    await self.wait_for_document_settle(timeout=min(5.0, timeout))
                    continue
                raise DemoError(
                    f"Playwright helper `{name}` timed out after {timeout:.1f}s; "
                    f"diagnostics={self.diagnostic_tail()!r}"
                ) from error
            except Exception as error:
                if read_only_helper and attempt + 1 < attempts and looks_like_navigation_context_loss(error):
                    await self.wait_for_document_settle(timeout=min(5.0, timeout))
                    continue
                text = str(error) or repr(error)
                raise DemoError(f"Playwright helper `{name}` failed: {text}") from error
        raise DemoError(f"Playwright helper `{name}` failed")

    async def conversation_state_best_effort(self, *, timeout: float = 10.0) -> dict[str, Any]:
        try:
            state = await self.helper("conversationState", timeout=timeout)
        except DemoError as error:
            if is_retryable_read_only_helper_error(error):
                return {}
            raise
        return state if isinstance(state, dict) else {}

    async def wait_for_state(self, predicate: Callable[[dict[str, Any]], bool], *, timeout: float, label: str) -> dict[str, Any]:
        deadline = time.monotonic() + timeout
        last_state: dict[str, Any] | None = None
        last_error: BaseException | None = None
        reported_waitable_reasons: set[str] = set()
        while time.monotonic() < deadline:
            try:
                state = await self.helper("loginState", timeout=15)
                if isinstance(state, dict):
                    last_state = state
                    if predicate(state):
                        return state
                    blocking_reason = state.get("blockingReason")
                    if blocking_reason:
                        if is_waitable_blocking_reason(blocking_reason):
                            reason = str(blocking_reason)
                            if reason not in reported_waitable_reasons:
                                self.report(f"waiting for auth step: {reason}")
                                reported_waitable_reasons.add(reason)
                            await asyncio.sleep(1.0)
                            continue
                        return state
            except BaseException as error:  # noqa: BLE001 - preserve final context.
                last_error = error
            await asyncio.sleep(0.5)
        if isinstance(last_state, dict) and is_waitable_blocking_reason(last_state.get("blockingReason")):
            raise waitable_blocking_error(last_state)
        raise DemoError(
            f"timed out waiting for {label}; "
            f"last_state={last_state!r}; last_error={last_error!r}; diagnostics={self.diagnostic_tail()!r}"
        )

    async def wait_for_login_form_hydration(self, *, timeout: float) -> dict[str, Any]:
        try:
            return await self.wait_for_state(
                lambda value: bool(
                    value.get("loggedIn")
                    or value.get("hasPasswordInput")
                    or not value.get("hasEmailInput")
                    or value.get("loginFormHydrated")
                ),
                timeout=timeout,
                label="hydrated login form",
            )
        except DemoError as error:
            self.report(f"continue before hydration: {error}")
            state = await self.helper("loginState", timeout=15)
            return state if isinstance(state, dict) else {}

    async def wait_for_password_input_on_auth_page(self, *, timeout: float) -> dict[str, Any]:
        if self.page is None:
            raise DemoError("Playwright page is not initialized")
        deadline = time.monotonic() + timeout
        selector = 'input[type="password"], input[name="password"], #password'
        last_error: BaseException | None = None
        reloaded_after_missing_session = False
        while time.monotonic() < deadline:
            url = self.page.url
            check_auth_intermediate_url(url)
            if is_auth_password_url(url):
                try:
                    await self.page.locator(selector).first.wait_for(state="attached", timeout=2000)
                    return {
                        "loggedIn": False,
                        "url": self.page.url,
                        "hasEmailInput": False,
                        "hasPasswordInput": True,
                        "blockingReason": "",
                    }
                except BaseException as error:  # noqa: BLE001 - keep polling through auth page hydration.
                    last_error = error
                if not reloaded_after_missing_session:
                    try:
                        state = await self.helper("snapshot", timeout=5)
                    except BaseException as error:  # noqa: BLE001 - keep original wait error too.
                        last_error = error
                    else:
                        if isinstance(state, dict) and is_auth_password_missing_session_state(state):
                            self.report("reload auth password page after session cookie")
                            reloaded_after_missing_session = True
                            await self.page.reload(
                                wait_until="domcontentloaded",
                                timeout=int(max(5.0, deadline - time.monotonic()) * 1000),
                            )
                            continue
            else:
                try:
                    await self.page.wait_for_url("**/log-in/password", timeout=1000)
                except BaseException as error:  # noqa: BLE001 - report final context below.
                    last_error = error
            await asyncio.sleep(0.25)
        raise DemoError(
            "timed out waiting for auth password page; "
            f"url={self.page.url!r}; last_error={last_error!r}; diagnostics={self.diagnostic_tail()!r}"
        )

    async def wait_for_auth_password_runtime_ready(self, *, timeout: float) -> dict[str, Any]:
        deadline = time.monotonic() + timeout
        last_state: dict[str, Any] | None = None
        while time.monotonic() < deadline:
            state = await self.helper("loginState", timeout=10)
            if not isinstance(state, dict):
                await asyncio.sleep(0.5)
                continue
            last_state = state
            if not is_actionable_auth_password_state(state):
                return state
            cookie_names = set(state.get("cookieNames") or [])
            if {"iss_context", "rg_context"}.issubset(cookie_names):
                return state
            await asyncio.sleep(0.5)
        if isinstance(last_state, dict):
            self.report(f"continue before auth runtime cookies ready: cookies={last_state.get('cookieNames')!r}")
            return last_state
        return {}

    async def wait_for_state_after_email_submit(self, *, timeout: float, label: str) -> dict[str, Any]:
        if self.page is None:
            raise DemoError("Playwright page is not initialized")
        deadline = time.monotonic() + timeout
        last_state: dict[str, Any] | None = None
        last_error: BaseException | None = None
        while time.monotonic() < deadline:
            url = self.page.url
            check_auth_intermediate_url(url)
            if is_auth_password_url(url):
                return await self.wait_for_password_input_on_auth_page(
                    timeout=max(1.0, deadline - time.monotonic()),
                )
            try:
                state = await self.helper("loginState", timeout=5)
                if isinstance(state, dict):
                    last_state = state
                    check_human_gate(state)
                    if state.get("hasPasswordInput") or state.get("loggedIn"):
                        return state
            except BaseException as error:  # noqa: BLE001 - navigation can invalidate Runtime.evaluate.
                last_error = error
            await asyncio.sleep(0.5)
        raise DemoError(
            f"timed out waiting for {label}; "
            f"url={self.page.url!r}; last_state={last_state!r}; "
            f"last_error={last_error!r}; diagnostics={self.diagnostic_tail()!r}"
        )

    def email_submit_compat_error(self, state: dict[str, Any], original_error: BaseException) -> DemoError:
        url = str(state.get("url") or "")
        text = str(state.get("text") or "")
        return DemoError(
            "ChatGPT email submit did not advance to the password page under Moli. "
            "The page stayed on the SSR email form instead of running the auth frontend redirect "
            "to `https://auth.openai.com/log-in/password`. "
            f"url={url!r}; hasEmailInput={state.get('hasEmailInput')!r}; "
            f"hasPasswordInput={state.get('hasPasswordInput')!r}; "
            f"text={text[:240]!r}; first_error={original_error}; diagnostics={self.diagnostic_tail()!r}"
        )

    async def accept_cookie_consent(self) -> bool:
        if self.page is None:
            raise DemoError("Playwright page is not initialized")
        for label in ("Accept all", "Reject non-essential"):
            try:
                await self.page.get_by_text(label, exact=True).click(timeout=2000)
                self.report(f"cookie consent: {label}")
                return True
            except Exception:
                continue
        try:
            result = await self.helper("acceptCookieConsent", timeout=3)
            if isinstance(result, dict) and result.get("ok"):
                self.report(f"cookie consent: {result.get('text') or 'accepted'}")
                return True
        except Exception:
            pass
        return False

    async def click_login_native(self) -> None:
        if self.page is None:
            raise DemoError("Playwright page is not initialized")
        selectors = [
            '[data-testid="login-button"]',
            'a[href*="/auth/login"]',
            'button:has-text("Log in")',
            'a:has-text("Log in")',
            'text=Log in',
        ]
        for selector in selectors:
            try:
                await self.page.locator(selector).first.click(timeout=3000)
                return
            except Exception:
                continue
        await self.page.goto(
            self.absolute_url("/auth/login"),
            wait_until="domcontentloaded",
            timeout=int(min(30.0, self.args.login_timeout) * 1000),
        )

    async def wait_for_login_entry_quick(self, *, timeout: float) -> dict[str, Any]:
        deadline = time.monotonic() + timeout
        last_state: dict[str, Any] | None = None
        while time.monotonic() < deadline:
            try:
                state = await self.helper("loginState", timeout=5)
            except DemoError:
                await asyncio.sleep(0.25)
                continue
            if isinstance(state, dict):
                last_state = state
                if state.get("hasEmailInput") or state.get("hasPasswordInput") or state.get("loggedIn"):
                    return state
                if state.get("blockingReason"):
                    return state
            await asyncio.sleep(0.5)
        return last_state or {}

    async def navigate_to_login_form(self) -> dict[str, Any]:
        if self.page is None:
            raise DemoError("Playwright page is not initialized")
        self.report("open login form")
        await self.click_login_native()
        await self.accept_cookie_consent()
        state = await self.wait_for_login_entry_quick(timeout=min(6.0, self.args.login_timeout))
        if state.get("hasEmailInput") or state.get("hasPasswordInput") or state.get("loggedIn"):
            return state

        if state.get("hasLoginButton"):
            self.report("retry login after cookie consent")
            await self.click_login_native()
            state = await self.wait_for_login_entry_quick(timeout=min(6.0, self.args.login_timeout))
            if state.get("hasEmailInput") or state.get("hasPasswordInput") or state.get("loggedIn"):
                return state

        self.report("navigate directly to /auth/login")
        await self.page.goto(
            self.absolute_url("/auth/login"),
            wait_until="domcontentloaded",
            timeout=int(min(30.0, self.args.login_timeout) * 1000),
        )
        await self.accept_cookie_consent()
        return await self.wait_for_state(
            lambda value: bool(value.get("hasEmailInput") or value.get("hasPasswordInput") or value.get("loggedIn")),
            timeout=min(30.0, self.args.login_timeout),
            label="login form",
        )

    async def fill_email_native(self, email: str) -> None:
        if self.page is None:
            raise DemoError("Playwright page is not initialized")
        await self.page.locator('input[type="email"], input[name="email"], #email').first.fill(email, timeout=10_000)
        last_error: BaseException | None = None
        try:
            await self.page.locator("button").filter(has_text=re.compile(r"^Continue$")).first.click(timeout=5000)
            return
        except Exception as error:
            last_error = error
        for selector in ('input[type="submit"]',):
            try:
                await self.page.locator(selector).filter(has_text=re.compile(r"^Continue$")).first.click(timeout=5000)
                return
            except Exception as error:
                last_error = error
        try:
            await self.page.locator('input[type="email"], input[name="email"], #email').first.press("Enter", timeout=3000)
            return
        except Exception as error:
            last_error = error
        raise DemoError(f"failed to submit email form with Playwright locators: {last_error!r}")

    async def fill_password_native(self, password: str) -> None:
        if self.page is None:
            raise DemoError("Playwright page is not initialized")
        await self.page.locator('input[type="password"], input[name="password"], #password').first.fill(
            password,
            timeout=10_000,
        )
        last_error: BaseException | None = None
        for label in ("Log in", "Continue"):
            try:
                await self.page.locator("button").filter(has_text=re.compile(rf"^{re.escape(label)}$")).first.click(
                    timeout=5000,
                )
                return
            except Exception as error:
                last_error = error
        try:
            await self.page.locator('input[type="password"], input[name="password"], #password').first.press(
                "Enter",
                timeout=3000,
            )
            return
        except Exception as error:
            last_error = error
        raise DemoError(f"failed to submit password form with Playwright locators: {last_error!r}")

    async def fill_prompt_native(self, prompt: str) -> dict[str, Any]:
        if self.page is None:
            raise DemoError("Playwright page is not initialized")
        selectors = [
            'div.ProseMirror[contenteditable="true"]',
            '[contenteditable="plaintext-only"]',
            '[contenteditable="true"][id="prompt-textarea"]',
            '[contenteditable="true"][data-testid*="prompt" i]',
            'form [contenteditable="true"]',
            'textarea[name="prompt-textarea"]',
            'textarea[data-testid*="prompt" i]',
            'textarea[placeholder*="Message" i]',
            '#prompt-textarea',
        ]
        last_error: BaseException | None = None
        for selector in selectors:
            locator = self.page.locator(selector).first
            try:
                await locator.wait_for(state="visible", timeout=2500)
            except Exception as error:
                last_error = error
                continue
            try:
                await locator.fill(prompt, timeout=5000)
                await asyncio.sleep(0.1)
                state = await self.helper("conversationState", timeout=5)
                if isinstance(state, dict) and state.get("sendUsable"):
                    return {"ok": True, "method": "locator.fill", "selector": selector, "state": state}
            except Exception as error:
                last_error = error

            try:
                await locator.click(timeout=5000)
                await self.page.keyboard.press("Control+A")
                await self.page.keyboard.press("Backspace")
                await self.page.keyboard.insert_text(prompt)
                await asyncio.sleep(0.1)
                state = await self.helper("conversationState", timeout=5)
                if isinstance(state, dict) and state.get("sendUsable"):
                    return {"ok": True, "method": "keyboard.insert_text", "selector": selector, "state": state}
                return {
                    "ok": False,
                    "reason": "send-button-not-usable-after-native-input",
                    "selector": selector,
                    "state": state,
                }
            except Exception as error:
                last_error = error
                continue
        raise DemoError(f"failed to fill prompt with Playwright native input: {last_error!r}")

    async def click_send_prompt_native(self) -> dict[str, Any]:
        if self.page is None:
            raise DemoError("Playwright page is not initialized")
        selectors = [
            'button[data-testid="send-button"]',
            'button[aria-label*="Send" i]',
            'form button[type="submit"]',
            'button:has-text("Send")',
        ]
        last_error: BaseException | None = None
        for selector in selectors:
            locator = self.page.locator(selector).first
            try:
                await locator.click(timeout=5000)
                return {"ok": True, "method": "locator.click", "selector": selector}
            except Exception as error:
                last_error = error
            try:
                await locator.click(timeout=3000, force=True)
                return {"ok": True, "method": "locator.click(force)", "selector": selector}
            except Exception as error:
                last_error = error
        raise DemoError(f"failed to click send button with Playwright locators: {last_error!r}")

    async def fill_password_with_helper_after_native_error(
        self,
        password: str,
        native_error: BaseException,
        *,
        timeout: float,
    ) -> None:
        try:
            await self.helper("fillPassword", password, timeout=timeout)
        except DemoError as helper_error:
            if looks_like_navigation_context_loss(helper_error):
                self.report("password submit triggered navigation")
            elif looks_like_submit_timeout(helper_error):
                self.report("password submit timed out; checking page state")
            else:
                raise helper_error from native_error

    async def maybe_retry_password_submit(self, password: str, *, timeout: float) -> None:
        if self.page is None:
            raise DemoError("Playwright page is not initialized")
        try:
            state = await self.helper("loginState", timeout=10)
        except DemoError as state_error:
            if looks_like_navigation_context_loss(state_error):
                self.report("password submit still navigating")
                return
            raise
        if not isinstance(state, dict) or not is_actionable_auth_password_state(state):
            if isinstance(state, dict) and is_waitable_blocking_reason(state.get("blockingReason")):
                self.report(f"waiting for auth step: {state.get('blockingReason')}")
                return
            check_human_gate(state if isinstance(state, dict) else {})
            return
        self.report("retry password submit with page helper")
        try:
            await self.helper("fillPassword", password, timeout=timeout)
        except DemoError as helper_error:
            if looks_like_navigation_context_loss(helper_error):
                self.report("password submit triggered navigation")
            elif looks_like_submit_timeout(helper_error):
                self.report("password submit timed out; checking page state")
            else:
                raise

    async def click_auth_try_again(self) -> bool:
        if self.page is None:
            raise DemoError("Playwright page is not initialized")
        candidates = [
            self.page.get_by_role("button", name=re.compile(r"^try again$", re.IGNORECASE)),
            self.page.locator("button, a").filter(has_text=re.compile(r"^Try again$", re.IGNORECASE)),
            self.page.get_by_text(re.compile(r"^Try again$", re.IGNORECASE)),
        ]
        for candidate in candidates:
            try:
                await candidate.first.click(timeout=5000)
                self.report("retry auth password page after transient error")
                await self.wait_for_document_settle(timeout=5)
                return True
            except Exception:
                continue
        return False

    async def click_login_with_one_time_code(self) -> bool:
        if self.page is None:
            raise DemoError("Playwright page is not initialized")
        pattern = re.compile(r"(one[- ]time code|email code|log in with.*code)", re.IGNORECASE)
        candidates = [
            self.page.get_by_role("button", name=pattern),
            self.page.get_by_role("link", name=pattern),
            self.page.locator("button, a").filter(has_text=pattern),
            self.page.get_by_text(pattern),
        ]
        for candidate in candidates:
            try:
                await candidate.first.click(timeout=5000)
                self.report("switch auth to one-time code")
                await self.wait_for_document_settle(timeout=5)
                return True
            except Exception:
                continue
        return False

    def has_auth_code_source(self) -> bool:
        provider = getattr(self.args, "auth_code_provider", None)
        return bool(
            str(getattr(self.args, "auth_code", "") or "")
            or bool(getattr(self.args, "auth_code_stdin", False))
            or bool(getattr(self.args, "auth_code_prompt", False))
            or callable(provider)
        )

    async def read_auth_code(self) -> str:
        if self._auth_code_cache is not None:
            return self._auth_code_cache
        raw = str(getattr(self.args, "auth_code", "") or "")
        if not raw and bool(getattr(self.args, "auth_code_stdin", False)):
            self.report("read auth code from stdin")
            raw = await asyncio.to_thread(sys.stdin.readline)
        if not raw and bool(getattr(self.args, "auth_code_prompt", False)):
            if not sys.stdin.isatty():
                raise DemoError("--auth-code-prompt requires an interactive terminal")
            raw = await asyncio.to_thread(getpass.getpass, "ChatGPT auth code: ")
        if not raw:
            provider = getattr(self.args, "auth_code_provider", None)
            if callable(provider):
                self.report("wait for auth code input")
                raw = provider()
                if inspect.isawaitable(raw):
                    raw = await raw
        code = re.sub(r"[\s-]+", "", raw)
        if not code:
            raise DemoError("auth code is required; pass --auth-code, --auth-code-stdin, or --auth-code-prompt")
        self._auth_code_cache = code
        return code

    async def click_try_with_email_verification(self) -> bool:
        if self.page is None:
            raise DemoError("Playwright page is not initialized")
        candidates = [
            self.page.get_by_role("button", name=re.compile(r"try with email", re.IGNORECASE)),
            self.page.get_by_text(re.compile(r"try with email", re.IGNORECASE)),
            self.page.locator("button, a").filter(has_text=re.compile(r"try with email", re.IGNORECASE)),
        ]
        for candidate in candidates:
            try:
                await candidate.first.click(timeout=5000)
                self.report("switch auth to email verification")
                await self.wait_for_document_settle(timeout=5)
                return True
            except Exception:
                continue
        return False

    async def fill_auth_code_native(self, code: str) -> None:
        if self.page is None:
            raise DemoError("Playwright page is not initialized")
        selectors = [
            'input[autocomplete="one-time-code"]',
            'input[inputmode="numeric"]',
            'input[name*="code" i]',
            'input[id*="code" i]',
            'input[type="tel"]',
            'input[type="text"]',
        ]
        inputs: list[Any] = []
        for selector in selectors:
            locator = self.page.locator(selector)
            try:
                count = await locator.count()
            except Exception:
                continue
            for index in range(min(count, 12)):
                candidate = locator.nth(index)
                try:
                    if await candidate.is_visible(timeout=1000) and await candidate.is_enabled(timeout=1000):
                        inputs.append(candidate)
                except Exception:
                    continue
            if inputs:
                break
        if not inputs:
            state = await self.helper("snapshot", timeout=10)
            raise DemoError(f"auth code input not found; state={redact_snapshot(state)!r}")
        if len(inputs) > 1 and len(code) >= len(inputs):
            for index, candidate in enumerate(inputs):
                await candidate.fill(code[index], timeout=5000)
        else:
            await inputs[0].fill(code, timeout=10_000)
        last_error: BaseException | None = None
        for label in ("Continue", "Verify", "Submit", "Next"):
            try:
                await self.page.locator("button").filter(has_text=re.compile(rf"^{re.escape(label)}$")).first.click(
                    timeout=5000,
                )
                return
            except Exception as error:
                last_error = error
        try:
            await inputs[-1].press("Enter", timeout=3000)
            return
        except Exception as error:
            last_error = error
        raise DemoError(f"failed to submit auth code form with Playwright locators: {last_error!r}")

    async def wait_for_login_completion_after_password(self, password: str, *, timeout: float) -> dict[str, Any]:
        deadline = time.monotonic() + timeout
        started = time.monotonic()
        last_state: dict[str, Any] | None = None
        last_error: BaseException | None = None
        reported_reasons: set[str] = set()
        retried_password_submit = False
        tried_email_verification = False
        auth_code_attempts = 0
        auth_code_submitted_at: float | None = None
        reported_auth_code_wait = False
        last_progress_report = 0.0
        auth_error_retries = 0
        while time.monotonic() < deadline:
            try:
                state = await self.helper("loginState", timeout=15)
            except BaseException as error:  # noqa: BLE001 - navigation can invalidate Runtime.evaluate.
                last_error = error
                now = time.monotonic()
                if now - last_progress_report >= 15.0:
                    url = self.page.url if self.page is not None else ""
                    self.report(f"waiting for login state: url={redact_diagnostic_text(url)} error={error}")
                    last_progress_report = now
                await asyncio.sleep(0.5)
                continue
            if not isinstance(state, dict):
                await asyncio.sleep(0.5)
                continue
            last_state = state
            if state.get("loggedIn"):
                return state
            if (
                not retried_password_submit
                and time.monotonic() - started >= 8.0
                and is_actionable_auth_password_state(state)
            ):
                await self.maybe_retry_password_submit(password, timeout=45)
                retried_password_submit = True
                await asyncio.sleep(1.0)
                continue
            if is_retryable_auth_password_error_state(state) and auth_error_retries < 2:
                auth_error_retries += 1
                if await self.click_auth_try_again():
                    try:
                        retry_state = await self.wait_for_password_input_on_auth_page(timeout=20)
                    except DemoError:
                        await asyncio.sleep(1.0)
                        continue
                    if retry_state.get("hasPasswordInput"):
                        if bool(getattr(self.args, "try_email_verification", False)) and not tried_email_verification:
                            tried_email_verification = True
                            if await self.click_login_with_one_time_code():
                                await asyncio.sleep(1.0)
                                continue
                        await self.wait_for_auth_password_runtime_ready(timeout=15)
                        self.report("fill password after auth retry")
                        await self.helper("fillPassword", password, timeout=45)
                        await asyncio.sleep(1.5)
                        continue
            reason = state.get("blockingReason") or auth_blocking_reason_from_url(str(state.get("url") or ""))
            if reason == "device-approval":
                if bool(getattr(self.args, "try_email_verification", False)) and not tried_email_verification:
                    tried_email_verification = True
                    if await self.click_try_with_email_verification():
                        await asyncio.sleep(1.0)
                        continue
                if reason not in reported_reasons:
                    self.report("waiting for auth step: device-approval")
                    reported_reasons.add(str(reason))
                await asyncio.sleep(1.0)
                continue
            if is_code_blocking_reason(reason):
                now = time.monotonic()
                can_retry_code = bool(
                    getattr(self.args, "auth_code_stdin", False)
                    or getattr(self.args, "auth_code_prompt", False)
                    or callable(getattr(self.args, "auth_code_provider", None))
                )
                waiting_after_submit = (
                    auth_code_submitted_at is not None
                    and now - auth_code_submitted_at < 30.0
                )
                if auth_code_submitted_at is not None and waiting_after_submit:
                    if not reported_auth_code_wait:
                        self.report("waiting for auth code verification")
                        reported_auth_code_wait = True
                    await asyncio.sleep(1.0)
                    continue
                if auth_code_attempts > 0 and not can_retry_code:
                    raise DemoError(
                        "login stayed on the auth code step after submitting a code; "
                        f"state={redact_snapshot(state)!r}; diagnostics={self.diagnostic_tail()!r}"
                    )
                if auth_code_attempts > 0:
                    self._auth_code_cache = None
                    self.report("auth code still pending; read another auth code")
                if self.has_auth_code_source():
                    code = await self.read_auth_code()
                    self.report("fill auth code")
                    await self.fill_auth_code_native(code)
                    auth_code_attempts += 1
                    auth_code_submitted_at = time.monotonic()
                    reported_auth_code_wait = False
                    if bool(getattr(self.args, "debug_snapshot", False)):
                        try:
                            snapshot = await self.helper("snapshot", timeout=10)
                        except DemoError:
                            snapshot = {}
                        self.report(f"auth code submit state: {redact_snapshot(snapshot)!r}")
                    await asyncio.sleep(1.5)
                    continue
                raise DemoError(
                    f"login reached an auth code step ({reason}); "
                    "rerun with --auth-code, --auth-code-stdin, or --auth-code-prompt"
                )
            if reason:
                raise DemoError(f"login reached a verification step ({reason}); this demo does not bypass it")
            now = time.monotonic()
            if now - last_progress_report >= 15.0:
                text = redact_diagnostic_text(str(state.get("text") or "").replace("\n", " ")[:240])
                self.report(
                    "waiting for logged-in composer: "
                    f"url={redact_diagnostic_text(str(state.get('url') or ''))} "
                    f"hasPasswordInput={bool(state.get('hasPasswordInput'))} "
                    f"hasComposer={bool(state.get('hasComposer'))} "
                    f"hasLoginButton={bool(state.get('hasLoginButton'))} "
                    f"hasAccountCookie={bool(state.get('hasAccountCookie'))} "
                    f"cookies={state.get('cookieNames')!r} "
                    f"lastPasswordSubmit={redact_snapshot(state.get('lastPasswordSubmit'))!r} "
                    f"text={text!r}"
                )
                last_progress_report = now
            await asyncio.sleep(0.5)
        if isinstance(last_state, dict) and is_waitable_blocking_reason(last_state.get("blockingReason")):
            raise waitable_blocking_error(last_state)
        raise DemoError(
            "timed out waiting for logged-in ChatGPT composer after password submit; "
            f"last_state={redact_snapshot(last_state)!r}; "
            f"last_error={last_error!r}; diagnostics={self.diagnostic_tail()!r}"
        )

    def absolute_url(self, path: str) -> str:
        if path.startswith("http://") or path.startswith("https://"):
            return path
        return self.args.url.rstrip("/") + "/" + path.lstrip("/")

    async def navigate_to_initial_login_state(self) -> dict[str, Any]:
        if self.page is None:
            raise DemoError("Playwright page is not initialized")
        self.report(f"navigate: {self.args.url}")
        await self.page.goto(
            self.args.url,
            wait_until="domcontentloaded",
            timeout=int(self.args.login_timeout * 1000),
        )
        await self.accept_cookie_consent()
        return await self.helper("loginState", timeout=10)

    async def require_existing_session(self) -> None:
        state = await self.navigate_to_initial_login_state()
        if isinstance(state, dict) and state.get("loggedIn"):
            self.report("already logged in")
            return
        raise DemoError(
            "existing profile is not logged in; "
            "rerun with --email and --password-stdin or use a logged-in --profile-dir; "
            f"state={redact_snapshot(state)!r}"
        )

    async def login(self, email: str, password: str) -> None:
        state = await self.navigate_to_initial_login_state()
        if isinstance(state, dict) and state.get("loggedIn"):
            self.report("already logged in")
            return

        state = await self.navigate_to_login_form()
        check_human_gate(state)
        if state.get("loggedIn"):
            return

        if state.get("hasEmailInput"):
            if not state.get("loginFormHydrated"):
                self.report("wait for hydrated login form")
                state = await self.wait_for_login_form_hydration(
                    timeout=min(35.0, self.args.login_timeout),
                )
                check_human_gate(state)
                if state.get("loggedIn"):
                    return
                if state.get("hasPasswordInput"):
                    pass
                elif not state.get("hasEmailInput"):
                    state = await self.helper("loginState", timeout=15)

        if state.get("hasEmailInput"):
            self.report("fill email")
            await self.accept_cookie_consent()
            if state.get("loginFormHydrated"):
                try:
                    await self.helper("fillEmail", email, timeout=45)
                except DemoError as helper_error:
                    if looks_like_navigation_context_loss(helper_error):
                        self.report("email submit triggered navigation")
                    elif looks_like_submit_timeout(helper_error):
                        self.report("email submit timed out; checking page state")
                    else:
                        await self.fill_email_native(email)
            else:
                try:
                    await self.fill_email_native(email)
                except Exception as native_error:
                    try:
                        await self.helper("fillEmail", email, timeout=45)
                    except DemoError as helper_error:
                        if looks_like_navigation_context_loss(helper_error):
                            self.report("email submit triggered navigation")
                        elif looks_like_submit_timeout(helper_error):
                            self.report("email submit timed out; checking page state")
                        else:
                            raise helper_error from native_error
            await asyncio.sleep(1.5)
            try:
                state = await self.wait_for_state_after_email_submit(
                    timeout=min(45.0, self.args.login_timeout),
                    label="password form after native email submit",
                )
            except DemoError as first_error:
                check_auth_intermediate_url(self.page.url if self.page is not None else "")
                self.report("retry email submit after cookie consent")
                await self.accept_cookie_consent()
                try:
                    await self.helper("fillEmail", email, timeout=45)
                except DemoError as helper_error:
                    if looks_like_navigation_context_loss(helper_error):
                        self.report("email submit triggered navigation")
                    elif looks_like_submit_timeout(helper_error):
                        self.report("email submit timed out; checking page state")
                    else:
                        raise helper_error from first_error
                await asyncio.sleep(1.5)
                try:
                    state = await self.wait_for_state_after_email_submit(
                        timeout=min(45.0, self.args.login_timeout),
                        label="password form",
                    )
                except DemoError:
                    check_auth_intermediate_url(self.page.url if self.page is not None else "")
                    try:
                        current = await self.helper("loginState", timeout=15)
                    except DemoError as state_error:
                        current = {
                            "url": self.page.url if self.page is not None else "",
                            "stateError": str(state_error),
                        }
                    if isinstance(current, dict):
                        raise self.email_submit_compat_error(current, first_error) from first_error
                    raise
            check_human_gate(state)

        if state.get("loggedIn"):
            return
        if not state.get("hasPasswordInput"):
            raise DemoError(f"password input did not appear; state={redact_snapshot(state)!r}")

        await self.wait_for_auth_password_runtime_ready(timeout=15)
        self.report("fill password")
        try:
            await self.helper("fillPassword", password, timeout=45)
        except DemoError as helper_error:
            if looks_like_navigation_context_loss(helper_error):
                self.report("password submit triggered navigation")
            elif looks_like_submit_timeout(helper_error):
                self.report("password submit timed out; checking page state")
            else:
                try:
                    await self.fill_password_native(password)
                except Exception as native_error:
                    raise helper_error from native_error
        await asyncio.sleep(1.5)
        state = await self.wait_for_login_completion_after_password(
            password,
            timeout=max(1.0, self.args.login_timeout),
        )
        check_human_gate(state)
        if not state.get("loggedIn"):
            raise DemoError(f"login did not reach composer; state={redact_snapshot(state)!r}")

    def reload_recovery_enabled(self) -> bool:
        return not bool(getattr(self.args, "no_reload_recovery", False))

    async def ask_once(self, prompt: str, answer_update: AnswerUpdate = None) -> AnswerResult:
        before = await self.conversation_state_best_effort(timeout=10)
        self.report("send prompt")
        try:
            result = await self.fill_prompt_native(prompt)
            if not result.get("ok"):
                self.report(f"native prompt input incomplete: {result.get('reason') or 'unknown'}")
                raise DemoError(
                    f"native prompt input did not make send usable: {redact_snapshot(result)!r}"
                )
            self.report(f"prompt input: {result.get('method')}")
        except Exception as native_error:
            self.report("retry prompt input with page helper")
            result = await self.helper("fillPrompt", prompt, timeout=10)
            if not isinstance(result, dict) or not result.get("ok"):
                raise DemoError(f"failed to fill prompt: {redact_snapshot(result)!r}") from native_error
        send_deadline = time.monotonic() + min(30.0, max(5.0, self.args.answer_timeout / 3))
        send_state: dict[str, Any] = {}
        last_send_error: BaseException | None = None
        while time.monotonic() < send_deadline:
            try:
                state = await self.helper("conversationState", timeout=10)
            except DemoError as error:
                if not is_retryable_read_only_helper_error(error):
                    raise
                last_send_error = error
                await asyncio.sleep(0.5)
                continue
            except BaseException as error:  # noqa: BLE001 - preserve final diagnostics.
                last_send_error = error
                await asyncio.sleep(0.5)
                continue
            if isinstance(state, dict):
                send_state = state
                reason = state.get("blockingReason")
                if reason:
                    raise DemoError(
                        f"ChatGPT reached a blocking step before send ({reason}); "
                        f"state={redact_snapshot(state)!r}"
                    )
                if state.get("sendUsable"):
                    break
            await asyncio.sleep(0.5)
        else:
            raise DemoError(
                "timed out waiting for usable send button; "
                f"state={redact_snapshot(send_state)!r}; last_error={last_send_error!r}; "
                f"diagnostics={self.diagnostic_tail()!r}"
            )
        before_click_url = self.page.url if self.page is not None else ""
        try:
            result = await self.click_send_prompt_native()
            self.report(f"send click: {result.get('method')}")
        except Exception as native_error:
            if isinstance(native_error, DemoError) and looks_like_navigation_context_loss(native_error):
                self.report("send prompt triggered navigation")
                await self.wait_for_document_settle(timeout=5)
                result = {"ok": True, "method": "navigation-context-loss"}
            else:
                post_click_state = await self.conversation_state_best_effort(timeout=5)
                post_click_url = self.page.url if self.page is not None else ""
                click_appears_submitted = bool(
                    (post_click_url and post_click_url != before_click_url and "/c/" in post_click_url)
                    or post_click_state.get("isGenerating")
                    or post_click_state.get("stopButton")
                )
                if click_appears_submitted:
                    self.report("send click continued after native click error")
                    result = {"ok": True, "method": "locator.click(post-state)"}
                else:
                    self.report("retry send click with page helper")
                    try:
                        result = await self.helper("clickSendPrompt", timeout=10)
                    except DemoError as error:
                        if not looks_like_navigation_context_loss(error):
                            raise
                        self.report("send prompt triggered navigation")
                        await self.wait_for_document_settle(timeout=5)
                        result = {"ok": True, "method": "helper-navigation-context-loss"}
                    else:
                        if not isinstance(result, dict) or not result.get("ok"):
                            raise DemoError(
                                f"failed to click send button: result={redact_snapshot(result)!r}; "
                                f"state={redact_snapshot(send_state)!r}"
                            ) from native_error
        else:
            if not isinstance(result, dict) or not result.get("ok"):
                raise DemoError(
                    f"failed to click send button: result={redact_snapshot(result)!r}; "
                    f"state={redact_snapshot(send_state)!r}"
                )
        self.report("waiting for response")

        deadline = time.monotonic() + self.args.answer_timeout
        before_count = int(before.get("assistantCount") or 0)
        before_text = str(before.get("latestAssistantText") or "")
        last = ""
        last_error: BaseException | None = None
        reloaded_conversation = False
        reload_after = min(90.0, max(45.0, self.args.answer_timeout / 3))
        wait_started = time.monotonic()
        stable_since: float | None = None
        idle_no_render_since: float | None = None
        while time.monotonic() < deadline:
            try:
                state = await self.helper("conversationState", timeout=10)
            except DemoError as error:
                if not is_retryable_read_only_helper_error(error):
                    raise
                last_error = error
                await asyncio.sleep(1.0)
                continue
            if not isinstance(state, dict):
                state = {}
            reason = state.get("blockingReason")
            if reason:
                raise DemoError(f"ChatGPT reached a blocking step while waiting for answer ({reason})")
            current = str(state.get("latestAssistantText") or "")
            count = int(state.get("assistantCount") or 0)
            is_generating = bool(state.get("isGenerating"))
            if (
                current
                and not is_transient_assistant_text(current)
                and (count > before_count or current != before_text)
            ):
                if current != last:
                    last = current
                    stable_since = time.monotonic()
                    if answer_update is not None:
                        answer_update(current)
                elif stable_since is not None and time.monotonic() - stable_since >= 2.0 and not is_generating:
                    await self.write_live_trace_summary("live-dom")
                    return AnswerResult(text=current, source="live-dom")
            elif last and stable_since is not None and not is_generating and time.monotonic() - stable_since >= 2.0:
                await self.write_live_trace_summary("live-dom")
                return AnswerResult(text=last, source="live-dom")
            now = time.monotonic()
            if (
                self.reload_recovery_enabled()
                and not reloaded_conversation
                and not last
                and not current
                and not is_generating
                and count <= before_count
                and "/c/" in str(state.get("url") or "")
            ):
                if idle_no_render_since is None:
                    idle_no_render_since = now
                    self.report("conversation idle without response DOM; checking persisted response")
                elif now - idle_no_render_since >= 3.0:
                    reloaded_conversation = True
                    recovered = await self.reload_current_conversation_for_response(
                        before_count,
                        before_text,
                        timeout=max(1.0, min(90.0, deadline - now)),
                        answer_update=answer_update,
                    )
                    if recovered is not None:
                        return recovered
            elif (
                not self.reload_recovery_enabled()
                and not last
                and not current
                and not is_generating
                and count <= before_count
                and "/c/" in str(state.get("url") or "")
            ):
                if idle_no_render_since is None:
                    idle_no_render_since = now
                    self.report("conversation idle without response DOM; reload recovery disabled")
            else:
                idle_no_render_since = None
            if (
                self.reload_recovery_enabled()
                and not reloaded_conversation
                and not last
                and "/c/" in str(state.get("url") or "")
                and now - wait_started >= reload_after
            ):
                reloaded_conversation = True
                recovered = await self.reload_current_conversation_for_response(
                    before_count,
                    before_text,
                    timeout=max(1.0, min(90.0, deadline - now)),
                    answer_update=answer_update,
                )
                if recovered is not None:
                    return recovered
            await asyncio.sleep(1.0)
        try:
            final_state = await self.helper("conversationState", timeout=10)
        except DemoError as error:
            final_state = {}
            last_error = error
        live_trace = summarize_live_trace(await self.live_trace_snapshot_best_effort(timeout=5))
        await self.write_live_trace_summary("timeout", live_trace)
        raise DemoError(
            "timed out waiting for ChatGPT response; "
            f"last={last[-1000:]!r}; state={redact_snapshot(final_state)!r}; "
            f"reload_recovery={self.reload_recovery_enabled()}; "
            f"live_trace={redact_snapshot(live_trace)!r}; "
            f"last_error={last_error!r}; diagnostics={self.diagnostic_tail()!r}"
        )

    async def reload_current_conversation_for_response(
        self,
        before_count: int,
        before_text: str,
        *,
        timeout: float,
        answer_update: AnswerUpdate = None,
    ) -> AnswerResult | None:
        if self.page is None:
            return None
        url = str(self.page.url or "")
        if "/c/" not in url:
            return None
        self.report("reload conversation to verify persisted response")
        try:
            await self.page.goto(url, wait_until="domcontentloaded", timeout=30_000)
        except Exception as error:
            if not looks_like_navigation_context_loss(error):
                detail = redact_diagnostic_text(str(error))
                self.report(f"conversation reload did not finish cleanly: {detail}")
        deadline = time.monotonic() + timeout
        latest = ""
        stable_since: float | None = None
        while time.monotonic() < deadline:
            try:
                state = await self.helper("conversationState", timeout=15)
            except DemoError as error:
                if not is_retryable_read_only_helper_error(error):
                    raise
                await asyncio.sleep(1.0)
                continue
            if not isinstance(state, dict):
                state = {}
            reason = state.get("blockingReason")
            if reason:
                raise DemoError(
                    f"ChatGPT reached a blocking step after conversation reload ({reason})"
                )
            current = str(state.get("latestAssistantText") or "")
            count = int(state.get("assistantCount") or 0)
            is_generating = bool(state.get("isGenerating"))
            if (
                current
                and not is_transient_assistant_text(current)
                and (count > before_count or current != before_text)
            ):
                if current != latest:
                    latest = current
                    stable_since = time.monotonic()
                    if answer_update is not None:
                        answer_update(current)
                elif stable_since is not None and time.monotonic() - stable_since >= 2.0 and not is_generating:
                    await self.write_live_trace_summary("persisted-reload")
                    return AnswerResult(text=current, source="persisted-reload")
            elif latest and stable_since is not None and not is_generating and time.monotonic() - stable_since >= 2.0:
                await self.write_live_trace_summary("persisted-reload")
                return AnswerResult(text=latest, source="persisted-reload")
            await asyncio.sleep(1.0)
        return None


def check_human_gate(state: dict[str, Any]) -> None:
    reason = state.get("blockingReason") or auth_blocking_reason_from_url(str(state.get("url") or ""))
    if reason:
        raise DemoError(f"login reached a verification step ({reason}); this demo does not bypass it")
