use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use stellar_xdr::curr::{ScSpecEntry, ScSpecTypeDef};

#[derive(Debug, Clone)]
pub struct Config {
    pub wasm: PathBuf,
    pub spec_json: Option<PathBuf>,
    pub typescript: Option<PathBuf>,
    pub python: Option<PathBuf>,
    pub check: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ContractSpec {
    pub functions: Vec<FunctionSpec>,
    pub structs: Vec<StructSpec>,
    pub enums: Vec<EnumSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FunctionSpec {
    pub name: String,
    pub doc: String,
    pub inputs: Vec<FieldSpec>,
    pub output: TypeSpec,
    pub readonly: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FieldSpec {
    pub name: String,
    pub doc: String,
    pub type_: TypeSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StructSpec {
    pub name: String,
    pub doc: String,
    pub fields: Vec<FieldSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnumSpec {
    pub name: String,
    pub doc: String,
    pub cases: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TypeSpec {
    Void,
    Bool,
    Number {
        soroban: String,
    },
    BigInt {
        soroban: String,
    },
    String,
    Address,
    Bytes,
    Option {
        value: Box<TypeSpec>,
    },
    Result {
        ok: Box<TypeSpec>,
        error: Box<TypeSpec>,
    },
    Vec {
        element: Box<TypeSpec>,
    },
    Map {
        key: Box<TypeSpec>,
        value: Box<TypeSpec>,
    },
    Tuple {
        values: Vec<TypeSpec>,
    },
    Udt {
        name: String,
    },
    Val,
}

pub fn run(config: Config) -> Result<(), String> {
    let wasm = fs::read(&config.wasm)
        .map_err(|err| format!("failed to read {}: {err}", config.wasm.display()))?;
    let entries = soroban_spec::read::from_wasm(&wasm)
        .map_err(|err| format!("failed to read WASM contract spec: {err}"))?;
    let spec = spec_from_entries(&entries);

    write_or_check(
        config.spec_json.as_deref(),
        &serde_json::to_string_pretty(&spec).unwrap(),
        config.check,
    )?;
    write_or_check(
        config.typescript.as_deref(),
        &generate_typescript(&spec),
        config.check,
    )?;
    write_or_check(
        config.python.as_deref(),
        &generate_python(&spec),
        config.check,
    )?;
    Ok(())
}

fn write_or_check(path: Option<&Path>, content: &str, check: bool) -> Result<(), String> {
    let Some(path) = path else {
        return Ok(());
    };
    if check {
        let existing = fs::read_to_string(path)
            .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        if existing != content {
            return Err(format!("{} is not up to date", path.display()));
        }
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
    }
    fs::write(path, content).map_err(|err| format!("failed to write {}: {err}", path.display()))
}

pub fn spec_from_entries(entries: &[ScSpecEntry]) -> ContractSpec {
    let mut functions = Vec::new();
    let mut structs = Vec::new();
    let mut enums = Vec::new();

    for entry in entries {
        match entry {
            ScSpecEntry::FunctionV0(function) => {
                let name = function.name.to_string();
                functions.push(FunctionSpec {
                    readonly: is_readonly(&name),
                    name,
                    doc: function.doc.to_string(),
                    inputs: function
                        .inputs
                        .iter()
                        .map(|input| FieldSpec {
                            name: input.name.to_string(),
                            doc: input.doc.to_string(),
                            type_: type_from_xdr(&input.type_),
                        })
                        .collect(),
                    output: function
                        .outputs
                        .first()
                        .map(type_from_xdr)
                        .unwrap_or(TypeSpec::Void),
                });
            }
            ScSpecEntry::UdtStructV0(struct_) => structs.push(StructSpec {
                name: struct_.name.to_string(),
                doc: struct_.doc.to_string(),
                fields: struct_
                    .fields
                    .iter()
                    .map(|field| FieldSpec {
                        name: field.name.to_string(),
                        doc: field.doc.to_string(),
                        type_: type_from_xdr(&field.type_),
                    })
                    .collect(),
            }),
            ScSpecEntry::UdtEnumV0(enum_) => enums.push(EnumSpec {
                name: enum_.name.to_string(),
                doc: enum_.doc.to_string(),
                cases: enum_
                    .cases
                    .iter()
                    .map(|case| case.name.to_string())
                    .collect(),
            }),
            ScSpecEntry::UdtUnionV0(union_) => enums.push(EnumSpec {
                name: union_.name.to_string(),
                doc: union_.doc.to_string(),
                cases: union_
                    .cases
                    .iter()
                    .map(|case| format!("{case:?}"))
                    .collect(),
            }),
            ScSpecEntry::UdtErrorEnumV0(enum_) => enums.push(EnumSpec {
                name: enum_.name.to_string(),
                doc: enum_.doc.to_string(),
                cases: enum_
                    .cases
                    .iter()
                    .map(|case| case.name.to_string())
                    .collect(),
            }),
            ScSpecEntry::EventV0(_) => {}
        }
    }

    functions.sort_by(|a, b| a.name.cmp(&b.name));
    structs.sort_by(|a, b| a.name.cmp(&b.name));
    enums.sort_by(|a, b| a.name.cmp(&b.name));
    ContractSpec {
        functions,
        structs,
        enums,
    }
}

fn type_from_xdr(type_: &ScSpecTypeDef) -> TypeSpec {
    match type_ {
        ScSpecTypeDef::Void => TypeSpec::Void,
        ScSpecTypeDef::Bool => TypeSpec::Bool,
        ScSpecTypeDef::U32
        | ScSpecTypeDef::I32
        | ScSpecTypeDef::Timepoint
        | ScSpecTypeDef::Duration => TypeSpec::Number {
            soroban: type_.name().to_string(),
        },
        ScSpecTypeDef::U64
        | ScSpecTypeDef::I64
        | ScSpecTypeDef::U128
        | ScSpecTypeDef::I128
        | ScSpecTypeDef::U256
        | ScSpecTypeDef::I256 => TypeSpec::BigInt {
            soroban: type_.name().to_string(),
        },
        ScSpecTypeDef::String | ScSpecTypeDef::Symbol => TypeSpec::String,
        ScSpecTypeDef::Address | ScSpecTypeDef::MuxedAddress => TypeSpec::Address,
        ScSpecTypeDef::Bytes | ScSpecTypeDef::BytesN(_) => TypeSpec::Bytes,
        ScSpecTypeDef::Option(option) => TypeSpec::Option {
            value: Box::new(type_from_xdr(&option.value_type)),
        },
        ScSpecTypeDef::Result(result) => TypeSpec::Result {
            ok: Box::new(type_from_xdr(&result.ok_type)),
            error: Box::new(type_from_xdr(&result.error_type)),
        },
        ScSpecTypeDef::Vec(vec) => TypeSpec::Vec {
            element: Box::new(type_from_xdr(&vec.element_type)),
        },
        ScSpecTypeDef::Map(map) => TypeSpec::Map {
            key: Box::new(type_from_xdr(&map.key_type)),
            value: Box::new(type_from_xdr(&map.value_type)),
        },
        ScSpecTypeDef::Tuple(tuple) => TypeSpec::Tuple {
            values: tuple.value_types.iter().map(type_from_xdr).collect(),
        },
        ScSpecTypeDef::Udt(udt) => TypeSpec::Udt {
            name: udt.name.to_string(),
        },
        ScSpecTypeDef::Val | ScSpecTypeDef::Error => TypeSpec::Val,
    }
}

fn is_readonly(name: &str) -> bool {
    name.starts_with("get_")
        || name.starts_with("is_")
        || name.ends_with("_count")
        || matches!(
            name,
            "loan_status"
                | "vouch_exists"
                | "voucher_history"
                | "total_vouched"
                | "repayment_count"
                | "loan_count"
                | "default_count"
        )
}

pub fn generate_typescript(spec: &ContractSpec) -> String {
    let mut out = String::from(GENERATED_HEADER_TS);
    out.push_str("import { BASE_FEE, Contract, Keypair, TransactionBuilder, nativeToScVal, scValToNative, rpc } from '@stellar/stellar-sdk';\n\n");
    out.push_str("export interface ClientConfig {\n  contractId: string;\n  rpcUrl: string;\n  networkPassphrase: string;\n  keypair: Keypair;\n}\n\n");

    for struct_ in &spec.structs {
        out.push_str(&format!("export interface {} {{\n", struct_.name));
        for field in &struct_.fields {
            out.push_str(&format!(
                "  {}: {};\n",
                camel_case(&field.name),
                ts_type(&field.type_)
            ));
        }
        out.push_str("}\n\n");
    }
    for enum_ in &spec.enums {
        if enum_.cases.is_empty() {
            out.push_str(&format!("export type {} = string;\n\n", enum_.name));
        } else {
            let cases = enum_
                .cases
                .iter()
                .map(|case| format!("'{}'", case))
                .collect::<Vec<_>>()
                .join(" | ");
            out.push_str(&format!("export type {} = {};\n\n", enum_.name, cases));
        }
    }

    out.push_str("export class QuorumCreditClient {\n");
    out.push_str("  private readonly rpc: rpc.Server;\n  private readonly contract: Contract;\n\n");
    out.push_str("  constructor(private readonly config: ClientConfig) {\n    this.rpc = new rpc.Server(config.rpcUrl);\n    this.contract = new Contract(config.contractId);\n  }\n\n");
    for function in &spec.functions {
        out.push_str(&ts_method(function));
    }
    out.push_str("  private async submitTransaction(tx: ReturnType<TransactionBuilder['build']>): Promise<string> {\n");
    out.push_str("    tx.sign(this.config.keypair);\n    const result = await this.rpc.sendTransaction(tx);\n    return result.hash;\n  }\n\n");
    out.push_str("  private async invoke(name: string, readonly: boolean, ...args: unknown[]): Promise<unknown> {\n");
    out.push_str("    const scArgs = args.map((arg) => nativeToScVal(arg as never));\n");
    out.push_str("    if (readonly) {\n      const tx = await this.buildTransaction(name, scArgs);\n      const result = await this.rpc.simulateTransaction(tx);\n      if ('error' in result && result.error) throw new Error(result.error);\n      return scValToNative(result.results?.[0]?.result.retval);\n    }\n");
    out.push_str("    const tx = await this.buildTransaction(name, scArgs);\n    return this.submitTransaction(tx);\n  }\n\n");
    out.push_str("  private async buildTransaction(name: string, args: ReturnType<typeof nativeToScVal>[]) {\n");
    out.push_str("    const account = await this.rpc.getAccount(this.config.keypair.publicKey());\n    return new TransactionBuilder(account, { fee: BASE_FEE, networkPassphrase: this.config.networkPassphrase })\n      .addOperation((this.contract.call as (...args: unknown[]) => never)(name, ...args))\n      .setTimeout(30)\n      .build();\n  }\n");
    out.push_str("}\n");
    out
}

fn ts_method(function: &FunctionSpec) -> String {
    let name = camel_case(&function.name);
    let args = function
        .inputs
        .iter()
        .map(|input| format!("{}: {}", camel_case(&input.name), ts_type(&input.type_)))
        .collect::<Vec<_>>()
        .join(", ");
    let arg_names = function
        .inputs
        .iter()
        .map(|input| camel_case(&input.name))
        .collect::<Vec<_>>()
        .join(", ");
    let return_type = if function.readonly {
        ts_type(&function.output)
    } else {
        "string".to_string()
    };
    format!(
        "  async {name}({args}): Promise<{return_type}> {{\n    return this.invoke('{}', {}, {}) as Promise<{return_type}>;\n  }}\n\n",
        function.name,
        function.readonly,
        arg_names
    )
}

fn ts_type(type_: &TypeSpec) -> String {
    match type_ {
        TypeSpec::Void => "void".to_string(),
        TypeSpec::Bool => "boolean".to_string(),
        TypeSpec::Number { .. } => "number".to_string(),
        TypeSpec::BigInt { .. } => "bigint | string".to_string(),
        TypeSpec::String | TypeSpec::Address => "string".to_string(),
        TypeSpec::Bytes => "Uint8Array | string".to_string(),
        TypeSpec::Option { value } => format!("{} | null", ts_type(value)),
        TypeSpec::Result { ok, .. } => ts_type(ok),
        TypeSpec::Vec { element } => format!("{}[]", ts_type(element)),
        TypeSpec::Map { key, value } => {
            format!("Record<{}, {}>", record_key_type(key), ts_type(value))
        }
        TypeSpec::Tuple { values } => format!(
            "[{}]",
            values.iter().map(ts_type).collect::<Vec<_>>().join(", ")
        ),
        TypeSpec::Udt { name } => name.clone(),
        TypeSpec::Val => "unknown".to_string(),
    }
}

fn record_key_type(type_: &TypeSpec) -> &'static str {
    match type_ {
        TypeSpec::Number { .. } | TypeSpec::BigInt { .. } => "number",
        _ => "string",
    }
}

pub fn generate_python(spec: &ContractSpec) -> String {
    let mut out = String::from(GENERATED_HEADER_PY);
    out.push_str("from __future__ import annotations\n\nimport importlib\nfrom dataclasses import dataclass\nfrom typing import Any, Optional\n\n");
    out.push_str("stellar_sdk: Any = importlib.import_module(\"stellar_sdk\")\n\n\n");
    out.push_str("@dataclass(frozen=True)\nclass ClientConfig:\n    contract_id: str\n    rpc_url: str\n    network_passphrase: str\n    keypair: Any\n\n\n");
    for struct_ in &spec.structs {
        out.push_str("@dataclass(frozen=True)\n");
        out.push_str(&format!("class {}:\n", struct_.name));
        if struct_.fields.is_empty() {
            out.push_str("    pass\n\n\n");
        } else {
            for field in &struct_.fields {
                out.push_str(&format!(
                    "    {}: {}\n",
                    snake_case(&field.name),
                    py_type(&field.type_)
                ));
            }
            out.push_str("\n\n");
        }
    }
    out.push_str("class QuorumCreditClient:\n");
    out.push_str("    def __init__(self, config: ClientConfig) -> None:\n        self.config = config\n        self.server: Any = stellar_sdk.SorobanServer(config.rpc_url)\n\n");
    for function in &spec.functions {
        out.push_str(&py_method(function));
    }
    out.push_str("    async def _invoke(self, name: str, readonly: bool, *args: object) -> Any:\n");
    out.push_str("        if readonly:\n            tx = await self._build_transaction(name, *args)\n            result = self.server.simulate_transaction(tx)\n            if getattr(result, \"error\", None):\n                raise RuntimeError(result.error)\n            results = getattr(result, \"results\", None) or []\n            return results[0].result.retval if results else None\n");
    out.push_str("        tx = await self._build_transaction(name, *args)\n        result = self.server.send_transaction(tx)\n        return str(result.hash)\n\n");
    out.push_str("    async def _build_transaction(self, name: str, *args: object) -> Any:\n");
    out.push_str("        account = self.server.load_account(self.config.keypair.public_key)\n        return (\n            stellar_sdk.TransactionBuilder(account, base_fee=\"100\", network_passphrase=self.config.network_passphrase)\n            .append_invoke_contract_function_op(self.config.contract_id, name, list(args))\n            .set_timeout(30)\n            .build()\n        )\n");
    out
}

fn py_method(function: &FunctionSpec) -> String {
    let args = function
        .inputs
        .iter()
        .map(|input| format!("{}: {}", snake_case(&input.name), py_type(&input.type_)))
        .collect::<Vec<_>>()
        .join(", ");
    let arg_names = function
        .inputs
        .iter()
        .map(|input| snake_case(&input.name))
        .collect::<Vec<_>>()
        .join(", ");
    let prefix = if args.is_empty() {
        String::new()
    } else {
        format!(", {args}")
    };
    let invoke_args = if arg_names.is_empty() {
        String::new()
    } else {
        format!(", {arg_names}")
    };
    let return_type = if function.readonly {
        py_type(&function.output)
    } else {
        "str".to_string()
    };
    if function.readonly {
        format!(
            "    async def {}(self{}) -> {}:\n        return await self._invoke(\"{}\", true{})\n\n",
            snake_case(&function.name),
            prefix,
            return_type,
            function.name,
            invoke_args
        )
    } else {
        format!(
            "    async def {}(self{}) -> str:\n        return str(await self._invoke(\"{}\", false{}))\n\n",
            snake_case(&function.name),
            prefix,
            function.name,
            invoke_args
        )
    }
}

fn py_type(type_: &TypeSpec) -> String {
    match type_ {
        TypeSpec::Void => "None".to_string(),
        TypeSpec::Bool => "bool".to_string(),
        TypeSpec::Number { .. } => "int".to_string(),
        TypeSpec::BigInt { .. } => "int | str".to_string(),
        TypeSpec::String | TypeSpec::Address => "str".to_string(),
        TypeSpec::Bytes => "bytes | str".to_string(),
        TypeSpec::Option { value } => format!("Optional[{}]", py_type(value)),
        TypeSpec::Result { ok, .. } => py_type(ok),
        TypeSpec::Vec { element } => format!("list[{}]", py_type(element)),
        TypeSpec::Map { key, value } => format!("dict[{}, {}]", py_type(key), py_type(value)),
        TypeSpec::Tuple { values } => format!(
            "tuple[{}]",
            values.iter().map(py_type).collect::<Vec<_>>().join(", ")
        ),
        TypeSpec::Udt { name } => name.clone(),
        TypeSpec::Val => "Any".to_string(),
    }
}

fn camel_case(name: &str) -> String {
    let mut parts = name.split('_');
    let mut out = parts.next().unwrap_or_default().to_string();
    for part in parts {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.push(first.to_ascii_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}

fn snake_case(name: &str) -> String {
    let mut out = String::new();
    for (idx, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if idx > 0 {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

pub fn expected_signature_map(spec: &ContractSpec) -> BTreeMap<String, Vec<String>> {
    spec.functions
        .iter()
        .map(|function| {
            (
                function.name.clone(),
                function
                    .inputs
                    .iter()
                    .map(|input| format!("{}:{:?}", input.name, input.type_))
                    .collect(),
            )
        })
        .collect()
}

const GENERATED_HEADER_TS: &str = "// Generated by tools/sdkgen. Do not edit by hand.\n";
const GENERATED_HEADER_PY: &str = "# Generated by tools/sdkgen. Do not edit by hand.\n";
