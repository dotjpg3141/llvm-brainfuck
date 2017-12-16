use bf::*;
use bf::BfInstruction::*;

#[test]
fn optimize_no_change() {

    let assert_no_change = |v: Vec<_>| assert_optimize(v.clone(), v);

    assert_no_change(vec![SetValue(5)]);
    assert_no_change(vec![AddValue(5)]);
    assert_no_change(vec![Input]);
    assert_no_change(vec![Output]);
    assert_no_change(vec![BeginLoop]);
    assert_no_change(vec![EndLoop]);

}

#[test]
fn optimize_add_set_value() {
    assert_optimize(vec![AddValue(0)], vec![]);
    assert_optimize(vec![AddValue(5), AddValue(3)], vec![AddValue(8)]);
    assert_optimize(vec![SetValue(5), AddValue(3)], vec![SetValue(8)]);
    assert_optimize(vec![SetValue(5), SetValue(3)], vec![SetValue(3)]);
    assert_optimize(vec![AddValue(5), SetValue(3)], vec![SetValue(3)]);
}

#[test]
fn optimize_loop() {
    assert_optimize(vec![EndLoop, AddValue(5)], vec![EndLoop, SetValue(5)]);
    assert_optimize(vec![EndLoop, SetValue(0)], vec![EndLoop]);
    assert_optimize(vec![SetValue(0), BeginLoop], vec![SetValue(0)]);
    assert_optimize(vec![EndLoop, BeginLoop], vec![EndLoop]);
}

fn assert_optimize(input: Vec<BfInstruction>, expected: Vec<BfInstruction>) {
    let actual = InstructionList::from_vec(input).list;
    assert_eq!(actual, expected);
}
