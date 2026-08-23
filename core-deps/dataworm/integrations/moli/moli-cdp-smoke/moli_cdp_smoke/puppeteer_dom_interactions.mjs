export async function activateCssElementHandle(page, selector) {
  const handle = await page.$(selector);
  if (!handle) {
    throw new Error(`Puppeteer CSS selector did not match an element: ${selector}`);
  }
  try {
    return await handle.evaluate(element => {
      element.click();
      return { id: element.id, tag: element.tagName };
    });
  } finally {
    await handle.dispose();
  }
}

export async function activateXPathElement(page, expression) {
  return await page.evaluate(xpath => {
    const result = document.evaluate(
      xpath,
      document,
      null,
      XPathResult.FIRST_ORDERED_NODE_TYPE,
      null,
    );
    const element = result.singleNodeValue;
    if (!(element instanceof HTMLElement)) {
      throw new Error(`Puppeteer XPath did not match an HTML element: ${xpath}`);
    }
    element.click();
    return { id: element.id, tag: element.tagName };
  }, expression);
}

export async function runPuppeteerDomInteractionSmoke(page) {
  await page.evaluate(() => {
    document.body.innerHTML = `
      <form id="fixture-form">
        <label for="name">Name</label>
        <input id="name" name="name">
        <label for="notes">Notes</label>
        <textarea id="notes" name="notes"></textarea>
        <label id="enabled-label" for="enabled">Enabled</label>
        <input id="enabled" name="enabled" type="checkbox" value="yes">
        <label for="delivery-pickup">Pickup</label>
        <input id="delivery-pickup" name="delivery" type="radio" value="pickup">
        <label for="delivery-mail">Mail</label>
        <input id="delivery-mail" name="delivery" type="radio" value="mail">
        <label for="flavor">Flavor</label>
        <select id="flavor" name="flavor">
          <option value="plain">Plain</option>
          <option value="vanilla">Vanilla</option>
        </select>
        <details id="advanced">
          <summary id="advanced-summary">Advanced</summary>
          <span>Advanced settings</span>
        </details>
        <button id="disabled" type="button" disabled>Disabled</button>
        <button id="submit" type="submit">Submit</button>
        <output id="result"></output>
      </form>
    `;

    window.__puppeteerFormEvents = [];
    window.__puppeteerDisabledClicks = 0;
    for (const control of document.querySelectorAll('input, textarea, select')) {
      for (const eventType of ['input', 'change']) {
        control.addEventListener(eventType, () => {
          window.__puppeteerFormEvents.push(`${control.id}:${eventType}`);
        });
      }
    }
    document.querySelector('#disabled').addEventListener('click', () => {
      window.__puppeteerDisabledClicks += 1;
    });
    document.querySelector('#fixture-form').addEventListener('submit', event => {
      event.preventDefault();
      const data = new FormData(event.currentTarget);
      document.querySelector('#result').textContent = JSON.stringify({
        name: data.get('name'),
        notes: data.get('notes'),
        enabled: data.get('enabled'),
        delivery: data.get('delivery'),
        flavor: data.get('flavor'),
        detailsOpen: document.querySelector('#advanced').open,
        disabledClicks: window.__puppeteerDisabledClicks,
        submitter: event.submitter?.id ?? null,
        events: window.__puppeteerFormEvents.slice(),
      });
    });
  });

  await page.type('#name', 'puppeteer user');
  await page.type('#notes', 'notes from keyboard');

  const checkboxActivation = await activateCssElementHandle(page, '#enabled-label');
  if (checkboxActivation.id !== 'enabled-label' || checkboxActivation.tag !== 'LABEL') {
    throw new Error(`unexpected CSS ElementHandle activation: ${JSON.stringify(checkboxActivation)}`);
  }

  const radioActivation = await page.$eval('#delivery-mail', element => {
    element.click();
    return { checked: element.checked, id: element.id, tag: element.tagName };
  });
  if (!radioActivation.checked || radioActivation.id !== 'delivery-mail' || radioActivation.tag !== 'INPUT') {
    throw new Error(`unexpected page.$eval radio activation: ${JSON.stringify(radioActivation)}`);
  }

  const selectedValues = await page.select('#flavor', 'vanilla');
  if (selectedValues.length !== 1 || selectedValues[0] !== 'vanilla') {
    throw new Error(`unexpected Puppeteer select result: ${JSON.stringify(selectedValues)}`);
  }

  const summaryActivation = await activateCssElementHandle(page, '#advanced-summary');
  if (summaryActivation.id !== 'advanced-summary' || summaryActivation.tag !== 'SUMMARY') {
    throw new Error(`unexpected summary activation: ${JSON.stringify(summaryActivation)}`);
  }

  const disabledState = await page.$eval('#disabled', element => {
    element.click();
    return { disabled: element.disabled, id: element.id, tag: element.tagName };
  });
  if (!disabledState.disabled || disabledState.id !== 'disabled' || disabledState.tag !== 'BUTTON') {
    throw new Error(`unexpected disabled button state: ${JSON.stringify(disabledState)}`);
  }

  const submitActivation = await activateXPathElement(
    page,
    '//form[@id="fixture-form"]//button[@type="submit"]',
  );
  if (submitActivation.id !== 'submit' || submitActivation.tag !== 'BUTTON') {
    throw new Error(`unexpected XPath submit activation: ${JSON.stringify(submitActivation)}`);
  }

  const formResult = await page.$eval('#result', node => node.textContent);
  const parsed = JSON.parse(formResult);
  const expected = {
    name: 'puppeteer user',
    notes: 'notes from keyboard',
    enabled: 'yes',
    delivery: 'mail',
    flavor: 'vanilla',
    detailsOpen: true,
    disabledClicks: 0,
    submitter: 'submit',
  };
  for (const [key, value] of Object.entries(expected)) {
    if (parsed[key] !== value) {
      throw new Error(`unexpected Puppeteer form ${key}: ${JSON.stringify(parsed)}`);
    }
  }
  for (const expectedEvent of [
    'name:input',
    'notes:input',
    'enabled:change',
    'delivery-mail:change',
    'flavor:input',
    'flavor:change',
  ]) {
    if (!parsed.events.includes(expectedEvent)) {
      throw new Error(`missing Puppeteer form event ${expectedEvent}: ${JSON.stringify(parsed.events)}`);
    }
  }

  return {
    controls: ['input', 'textarea', 'checkbox', 'radio', 'select', 'details', 'disabled-button', 'form'],
    selectors: ['css-handle', '$eval', 'page.select', 'xpath'],
  };
}
