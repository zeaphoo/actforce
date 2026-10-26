import { RuntimeContext, StoreContext } from './context.js'

export function AssistantRuntimeProvider({ runtime, children }) {
  if (!runtime) {
    throw new Error('<AssistantRuntimeProvider> requires a `runtime` prop (see useExternalStoreRuntime).')
  }
  return (
    <RuntimeContext.Provider value={runtime}>
      <StoreContext.Provider value={runtime._store}>
        {children}
      </StoreContext.Provider>
    </RuntimeContext.Provider>
  )
}
