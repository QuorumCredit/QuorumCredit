use sdkgen::{
    expected_signature_map, generate_python, generate_typescript, run, spec_from_entries, Config,
    ContractSpec, FunctionSpec, TypeSpec,
};
use std::{fs, path::Path};
use stellar_xdr::curr::{
    Limits, ScSpecEntry, ScSpecFunctionInputV0, ScSpecFunctionV0, ScSpecTypeDef, ScSpecTypeOption,
    ScSpecTypeUdt, ScSpecUdtEnumCaseV0, ScSpecUdtEnumV0, ScSpecUdtStructFieldV0, ScSpecUdtStructV0,
    WriteXdr,
};

#[test]
fn introspection_includes_all_function_entries() {
    let spec = spec_from_entries(&fixture_entries());
    let names = spec
        .functions
        .iter()
        .map(|f| f.name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["get_loan", "request_loan", "vouch"]);
}

#[test]
fn introspection_marks_getters_readonly() {
    let spec = spec_from_entries(&fixture_entries());
    assert!(
        spec.functions
            .iter()
            .find(|f| f.name == "get_loan")
            .unwrap()
            .readonly
    );
    assert!(
        !spec
            .functions
            .iter()
            .find(|f| f.name == "vouch")
            .unwrap()
            .readonly
    );
}

#[test]
fn introspection_preserves_function_input_types() {
    let signatures = expected_signature_map(&spec_from_entries(&fixture_entries()));
    assert!(signatures["vouch"]
        .iter()
        .any(|arg| arg.starts_with("stake:BigInt")));
    assert!(signatures["request_loan"]
        .iter()
        .any(|arg| arg.starts_with("loan_purpose:String")));
}

#[test]
fn introspection_preserves_struct_fields() {
    let spec = spec_from_entries(&fixture_entries());
    let loan = spec
        .structs
        .iter()
        .find(|s| s.name == "LoanRecord")
        .unwrap();
    assert_eq!(
        loan.fields
            .iter()
            .map(|f| f.name.as_str())
            .collect::<Vec<_>>(),
        vec!["borrower", "amount"]
    );
}

#[test]
fn introspection_preserves_enum_cases() {
    let spec = spec_from_entries(&fixture_entries());
    let status = spec.enums.iter().find(|e| e.name == "LoanStatus").unwrap();
    assert_eq!(status.cases, vec!["Active", "Repaid"]);
}

#[test]
fn typescript_generation_emits_contract_methods() {
    let ts = generate_typescript(&fixture_contract_spec());
    assert!(ts.contains("async vouch(voucher: string, borrower: string, stake: bigint | string, token: string): Promise<string>"));
    assert!(ts.contains("async getLoan(borrower: string): Promise<LoanRecord | null>"));
}

#[test]
fn typescript_generation_emits_udt_interfaces() {
    let ts = generate_typescript(&fixture_contract_spec());
    assert!(ts.contains("export interface LoanRecord"));
    assert!(ts.contains("borrower: string;"));
    assert!(ts.contains("amount: bigint | string;"));
}

#[test]
fn python_generation_emits_typed_contract_methods() {
    let py = generate_python(&fixture_contract_spec());
    assert!(py.contains(
        "async def vouch(self, voucher: str, borrower: str, stake: int | str, token: str) -> str"
    ));
    assert!(py.contains("async def get_loan(self, borrower: str) -> Optional[LoanRecord]"));
}

#[test]
fn python_generation_emits_dataclasses() {
    let py = generate_python(&fixture_contract_spec());
    assert!(py.contains("@dataclass(frozen=True)\nclass LoanRecord:"));
    assert!(py.contains("    borrower: str"));
    assert!(py.contains("    amount: int | str"));
}

#[test]
fn wasm_extractor_reads_contractspec_section_and_generates_clients() {
    let dir = tempfile::tempdir().unwrap();
    let wasm = dir.path().join("contract.wasm");
    let spec_json = dir.path().join("contract_spec.json");
    let ts = dir.path().join("client.ts");
    let py = dir.path().join("client.py");
    write_fixture_wasm(&wasm);

    run(Config {
        wasm,
        spec_json: Some(spec_json.clone()),
        typescript: Some(ts.clone()),
        python: Some(py.clone()),
        check: false,
    })
    .unwrap();

    assert!(fs::read_to_string(spec_json).unwrap().contains("\"vouch\""));
    assert!(fs::read_to_string(ts)
        .unwrap()
        .contains("async requestLoan"));
    assert!(fs::read_to_string(py)
        .unwrap()
        .contains("async def request_loan"));
}

#[test]
fn check_mode_accepts_up_to_date_generated_files() {
    let dir = tempfile::tempdir().unwrap();
    let wasm = dir.path().join("contract.wasm");
    let spec_json = dir.path().join("contract_spec.json");
    let ts = dir.path().join("client.ts");
    let py = dir.path().join("client.py");
    write_fixture_wasm(&wasm);
    let config = Config {
        wasm,
        spec_json: Some(spec_json),
        typescript: Some(ts),
        python: Some(py),
        check: false,
    };
    run(config.clone()).unwrap();
    run(Config {
        check: true,
        ..config
    })
    .unwrap();
}

#[test]
fn check_mode_rejects_stale_generated_files() {
    let dir = tempfile::tempdir().unwrap();
    let wasm = dir.path().join("contract.wasm");
    let ts = dir.path().join("client.ts");
    write_fixture_wasm(&wasm);
    fs::write(&ts, "stale").unwrap();
    let err = run(Config {
        wasm,
        spec_json: None,
        typescript: Some(ts),
        python: None,
        check: true,
    })
    .unwrap_err();
    assert!(err.contains("is not up to date"));
}

#[test]
fn regression_contract_change_is_reflected_after_regeneration() {
    let mut spec = fixture_contract_spec();
    let before = generate_typescript(&spec);
    spec.functions.push(FunctionSpec {
        name: "new_view".to_string(),
        doc: String::new(),
        inputs: vec![],
        output: TypeSpec::Bool,
        readonly: true,
    });
    let after = generate_typescript(&spec);
    assert!(!before.contains("newView"));
    assert!(after.contains("async newView(): Promise<boolean>"));
}

fn fixture_contract_spec() -> ContractSpec {
    spec_from_entries(&fixture_entries())
}

fn fixture_entries() -> Vec<ScSpecEntry> {
    vec![
        ScSpecEntry::FunctionV0(function(
            "vouch",
            vec![
                input("voucher", ScSpecTypeDef::Address),
                input("borrower", ScSpecTypeDef::Address),
                input("stake", ScSpecTypeDef::I128),
                input("token", ScSpecTypeDef::Address),
            ],
            ScSpecTypeDef::Result(Box::new(stellar_xdr::curr::ScSpecTypeResult {
                ok_type: Box::new(ScSpecTypeDef::Void),
                error_type: Box::new(ScSpecTypeDef::Udt(ScSpecTypeUdt {
                    name: "ContractError".try_into().unwrap(),
                })),
            })),
        )),
        ScSpecEntry::FunctionV0(function(
            "request_loan",
            vec![
                input("borrower", ScSpecTypeDef::Address),
                input("amount", ScSpecTypeDef::I128),
                input("loan_purpose", ScSpecTypeDef::String),
            ],
            ScSpecTypeDef::Void,
        )),
        ScSpecEntry::FunctionV0(function(
            "get_loan",
            vec![input("borrower", ScSpecTypeDef::Address)],
            ScSpecTypeDef::Option(Box::new(ScSpecTypeOption {
                value_type: Box::new(ScSpecTypeDef::Udt(ScSpecTypeUdt {
                    name: "LoanRecord".try_into().unwrap(),
                })),
            })),
        )),
        ScSpecEntry::UdtStructV0(ScSpecUdtStructV0 {
            doc: "".try_into().unwrap(),
            lib: "".try_into().unwrap(),
            name: "LoanRecord".try_into().unwrap(),
            fields: vec![
                ScSpecUdtStructFieldV0 {
                    doc: "".try_into().unwrap(),
                    name: "borrower".try_into().unwrap(),
                    type_: ScSpecTypeDef::Address,
                },
                ScSpecUdtStructFieldV0 {
                    doc: "".try_into().unwrap(),
                    name: "amount".try_into().unwrap(),
                    type_: ScSpecTypeDef::I128,
                },
            ]
            .try_into()
            .unwrap(),
        }),
        ScSpecEntry::UdtEnumV0(ScSpecUdtEnumV0 {
            doc: "".try_into().unwrap(),
            lib: "".try_into().unwrap(),
            name: "LoanStatus".try_into().unwrap(),
            cases: vec![
                ScSpecUdtEnumCaseV0 {
                    doc: "".try_into().unwrap(),
                    name: "Active".try_into().unwrap(),
                    value: 0,
                },
                ScSpecUdtEnumCaseV0 {
                    doc: "".try_into().unwrap(),
                    name: "Repaid".try_into().unwrap(),
                    value: 1,
                },
            ]
            .try_into()
            .unwrap(),
        }),
    ]
}

fn function(
    name: &str,
    inputs: Vec<ScSpecFunctionInputV0>,
    output: ScSpecTypeDef,
) -> ScSpecFunctionV0 {
    ScSpecFunctionV0 {
        doc: "".try_into().unwrap(),
        name: name.try_into().unwrap(),
        inputs: inputs.try_into().unwrap(),
        outputs: vec![output].try_into().unwrap(),
    }
}

fn input(name: &str, type_: ScSpecTypeDef) -> ScSpecFunctionInputV0 {
    ScSpecFunctionInputV0 {
        doc: "".try_into().unwrap(),
        name: name.try_into().unwrap(),
        type_,
    }
}

fn write_fixture_wasm(path: &Path) {
    let mut spec = Vec::new();
    for entry in fixture_entries() {
        spec.extend(entry.to_xdr(Limits::none()).unwrap());
    }

    let mut wasm = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0x00];
    let mut payload = Vec::new();
    let name = b"contractspecv0";
    write_leb(name.len() as u32, &mut payload);
    payload.extend_from_slice(name);
    payload.extend_from_slice(&spec);
    write_leb(payload.len() as u32, &mut wasm);
    wasm.extend_from_slice(&payload);
    fs::write(path, wasm).unwrap();
}

fn write_leb(mut value: u32, out: &mut Vec<u8>) {
    loop {
        let mut byte = (value & 0x7f) as u8;
        value >>= 7;
        if value != 0 {
            byte |= 0x80;
        }
        out.push(byte);
        if value == 0 {
            break;
        }
    }
}
