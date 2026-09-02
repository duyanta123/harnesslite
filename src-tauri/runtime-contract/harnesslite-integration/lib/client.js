window.__ModuleLoader__.load({
  id: '@duyanta123/harnesslite-integration',
  factory: () => {
    const module = { exports: {} }
    const exports = module.exports
    Object.defineProperty(exports, Symbol.toStringTag, { value: 'Module' })

    const inject = ['workspaces']

    function apply(ctx) {
      const desktop = window.harnessLite
      if (!desktop || !desktop.workspace || typeof desktop.workspace.onDrop !== 'function') return

      ctx.effect(() => desktop.workspace.onDrop((path) => {
        void desktop.workspace.validate(path).then((review) => {
          if (!review.allowed) throw new Error(review.reason || 'HarnessLite rejected this workspace')
          return ctx.workspaces.create({ path })
        }).then((workspace) => {
          ctx.workspaces.startSession(workspace.workspaceId)
        }).catch((reason) => {
          const body = reason instanceof Error ? reason.message : String(reason)
          void desktop.notify({ title: 'Workspace could not be added', body }).catch(() => {})
        })
      }), 'harnesslite: native workspace folder drop')
    }

    exports.apply = apply
    exports.inject = inject
    return module.exports
  },
})
