({
  array: (...items) => items,
  options: digest => [digest === undefined ? undefined : { content: digest }, undefined, undefined, undefined],
  bind: (deliver, id) => value => deliver(id, value),
})