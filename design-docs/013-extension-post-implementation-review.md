# P12 Extension Runtime 实现后审查

## 当前结论

**IMPLEMENTED — UNIFIED VERIFICATION PENDING**。

## 第一轮：边界审查

- Extension crate 无 Agent/Workflow/Planning 依赖。
- Capability 是上层稳定依赖，Provider 与 Extension 是可替换实现。
- 未把 Manifest-only Loader 宣称为可信沙箱，未越界实现 Marketplace/WASM/Remote。

## 第二轮：生命周期与失败审查

- 修复并发 enable/disable/execute 竞态：所有同 Extension 操作使用 RAII live guard。
- 无效 Host result、Host error 和结果提交失败均保持 durable Running 并返回 OutcomeUnknown。
- resume 增加完整 invocation hash，禁止只复用 request ID 却更换 input/actor。
- enable/disable 的 Host 与 Store 失败路径提供反向恢复，Manifest revision 离线切换。

## 第三轮：持久化与安全审查

- 五表、审计字段、注释、索引、无外键。
- Manifest/Capability/Provider 归属和结构列/JSON 双重校验。
- 默认权限 fail-closed，敏感配置禁止持久化，artifact load 校验 SHA-256。

## 遗留风险

- 同步本地 Loader/SQLite 适用于 P12.0 小规模单进程场景。
- 真正不可信扩展必须等待 WASM/Process Sandbox 阶段。
