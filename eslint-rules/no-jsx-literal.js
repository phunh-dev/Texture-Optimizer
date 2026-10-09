// Forbids user-visible text written directly in JSX. All UI text must come
// from the locale files through t('key'), so translators can edit it.
const TEXT_ATTRIBUTES = new Set(['title', 'placeholder', 'alt', 'aria-label', 'label', 'description'])
const hasLetters = (s) => /\p{L}/u.test(s)

const isJsxChild = (node) =>
  node.parent?.type === 'JSXExpressionContainer' &&
  (node.parent.parent?.type === 'JSXElement' || node.parent.parent?.type === 'JSXFragment')

export default {
  meta: {
    type: 'problem',
    docs: { description: 'Disallow hard-coded UI text in JSX; use t() with a locale key' },
    messages: { literal: 'Hard-coded UI text "{{text}}": move it to src/locales and use t().' },
    schema: [],
  },
  create(context) {
    const report = (node, text) => context.report({ node, messageId: 'literal', data: { text: text.trim() } })
    return {
      JSXText(node) {
        if (hasLetters(node.value)) report(node, node.value)
      },
      JSXAttribute(node) {
        const name = typeof node.name.name === 'string' ? node.name.name : ''
        if (!TEXT_ATTRIBUTES.has(name) || !node.value) return
        const value = node.value.type === 'JSXExpressionContainer' ? node.value.expression : node.value
        if (value.type === 'Literal' && typeof value.value === 'string' && hasLetters(value.value)) {
          report(node, value.value)
        }
        if (value.type === 'TemplateLiteral' && value.quasis.some((q) => hasLetters(q.value.cooked ?? ''))) {
          report(node, value.quasis.map((q) => q.value.cooked).join('…'))
        }
      },
      Literal(node) {
        if (typeof node.value === 'string' && isJsxChild(node) && hasLetters(node.value)) report(node, node.value)
      },
      TemplateLiteral(node) {
        if (isJsxChild(node) && node.quasis.some((q) => hasLetters(q.value.cooked ?? ''))) {
          report(node, node.quasis.map((q) => q.value.cooked).join('…'))
        }
      },
    }
  },
}
