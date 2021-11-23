import { h, append } from './helpers'
export default class Component extends HTMLElement {

  root = this.attachShadow({ mode: 'open' })
  base = append(this.root)(h('div', { className: 'Inside' }))
  add  = append(this.base)

  // broken due to:
  // https://stackoverflow.com/questions/40181683/failed-to-execute-createelement-on-document-the-result-must-not-have-childr
  addTo (x: any) {
    x.appendChild(this)
  }

  constructor () {
    super()
    this.inheritStyles()
  }

  /** Ugly hack to inherit inlined style elements from document */
  private inheritStyles() {
    const styles = Array.from(document.head.querySelectorAll('style'))
    for (const style of styles) {
      const el = this.add(document.createElement('style'))
      el.innerHTML = style.innerHTML
    }
  }

  static get observedAttributes () {
    return [ 'class' ]
  }

  attributeChangedCallback (name: string, _oldValue: any, newValue: any) {
    switch (name) {
      case 'class':
        //this.classList.add('Outside')
        this.base.className = newValue.replace('Outside', 'Inside')
        //this.base.classList.add('Inside')
        break
    }
  }

}

import { encode, decode } from './helpers'
export abstract class ContractComponent extends Component {
  #contract: any
  setup (Contract: any) {
    this.#contract = new Contract()
    this.#contract.init(encode(this.initMsg))
    console.log({has:Contract.has_querier_callback})
    this.#contract.querier_callback = (x: any) => {
      console.log({x})
      return ""
    }
    this.update()
  }
  abstract readonly initMsg: any
  abstract update (): void

  query (msg: any) {
    console.debug('query', msg)
    return decode(this.#contract.query(encode(msg)))
  }

  handle (sender: any, msg: any) {
    console.debug('handle', sender, msg)
    this.#contract.sender = encode(sender)
    return decode(this.#contract.handle(encode(msg)))
  }
}
