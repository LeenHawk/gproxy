((prompt, submit) => {
  let box = document.querySelector('ms-prompt-box');
  if (!box) {
    box = document.createElement('ms-prompt-box');
    document.body.appendChild(box);
    box.appendChild(document.createElement('textarea'));
    const run = document.createElement('ms-run-button');
    document.body.appendChild(run);
    run.appendChild(document.createElement('button'));
  }
  const textarea = box.querySelector('textarea');
  textarea.focus();
  const ch = String(prompt || 'a').charAt(0) || 'a';
  const code = 'Key' + ch.toUpperCase();
  const kc = ch.toUpperCase().charCodeAt(0) || 65;
  textarea.dispatchEvent(new KeyboardEvent('keydown', { key: ch, code, keyCode: kc, which: kc, bubbles: true, cancelable: true }));
  const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set;
  setter.call(textarea, prompt);
  textarea.dispatchEvent(new InputEvent('input', { bubbles: true, inputType: 'insertText', data: prompt }));
  textarea.dispatchEvent(new KeyboardEvent('keyup', { key: ch, code, keyCode: kc, which: kc, bubbles: true, cancelable: true }));
  textarea.dispatchEvent(new Event('change', { bubbles: true }));
  if (submit) document.querySelector('ms-run-button').querySelector('button').click();
  return textarea.value;
})