#![feature(const_trait_impl, const_ops, generic_const_exprs)]
#![allow(incomplete_features)]
use quest_polynomial::{Interval, RemezBuilder, StaticDegree, typed_function};

#[test]
fn typed_const_expression_and_independent_derivatives() {
    const F: quest_polynomial::Function<quest_polynomial::typed::Variable> = typed_function!(|x| x);
    assert_eq!(F.evaluate(0.25).unwrap(), 0.25);
    let f = typed_function!(|x| (1.0 + x * x).ln());
    let j = f.jet(0.5).unwrap();
    assert!((j.value - 1.25_f64.ln()).abs() < 1e-15);
    assert!((j.first - 0.8).abs() < 1e-15);
    assert!((j.second - 0.96).abs() < 1e-15);
    let bounds = f.jet_interval(Interval::new(0.49, 0.51).unwrap()).unwrap();
    assert!(bounds.first.lower() <= 0.8 && bounds.first.upper() >= 0.8);
    assert!(f.metadata().nodes >= 5);
}

#[test]
fn static_degree_controls_actual_export_dimensions() {
    let degree = StaticDegree::<3>::new();
    let result = RemezBuilder::new()
        .target(typed_function!(|x| x.exp()))
        .static_degree(degree)
        .domain(Interval::new(-1.0, 1.0).unwrap())
        .unwrap()
        .run()
        .unwrap();
    let coefficients: &[_; 4] = result.coefficients().unwrap();
    assert_eq!(
        coefficients.as_slice(),
        result.result().polynomial().coefficients()
    );
    assert!(result.result().error_bound().upper() < 0.006);
}

#[test]
fn const_arithmetic_and_metadata_are_structural() {
    const METADATA: quest_polynomial::ExpressionMetadata = {
        let f = typed_function!(|x| (1.0 + x * x).ln());
        f.static_metadata()
    };
    const DEGREE: StaticDegree<4> = StaticDegree::new();
    const COEFFICIENTS: usize = DEGREE.coefficient_count();
    const ALTERNATION: usize = DEGREE.alternation_count();
    assert_eq!(METADATA.nodes, 6);
    assert_eq!(METADATA.depth, 4);
    assert_eq!(METADATA.operations, 3);
    assert_eq!(METADATA.positive_jet_arguments, 1);
    assert_eq!(METADATA.nonzero_denominators, 0);
    assert_eq!((COEFFICIENTS, ALTERNATION), (5, 6));
}

#[test]
fn all_unary_rules_match_independent_analytic_derivatives() {
    fn check<E: quest_polynomial::Expression>(
        f: &quest_polynomial::Function<E>,
        expected: [f64; 3],
    ) {
        let jet = f.jet(0.5).unwrap();
        for (a, b) in [jet.value, jet.first, jet.second].into_iter().zip(expected) {
            assert!((a - b).abs() < 2e-14, "actual {a}, expected {b}");
        }
        let dynamic = f.to_dynamic().jet(0.5).unwrap();
        assert_eq!(
            [jet.value, jet.first, jet.second],
            [dynamic.value, dynamic.first, dynamic.second]
        );
        let interval = f.jet_interval(Interval::point(0.5).unwrap()).unwrap();
        for (bound, value) in [interval.value, interval.first, interval.second]
            .into_iter()
            .zip(expected)
        {
            assert!(bound.lower() <= value && value <= bound.upper());
        }
    }
    let x = 0.5_f64;
    check(&typed_function!(|x| x.exp()), [x.exp(), x.exp(), x.exp()]);
    check(&typed_function!(|x| x.ln()), [x.ln(), 2.0, -4.0]);
    check(&typed_function!(|x| x.sin()), [x.sin(), x.cos(), -x.sin()]);
    check(&typed_function!(|x| x.cos()), [x.cos(), -x.sin(), -x.cos()]);
    check(
        &typed_function!(|x| x.sqrt()),
        [x.sqrt(), 0.5 / x.sqrt(), -0.25 / (x * x.sqrt())],
    );
    check(
        &typed_function!(|x| -(x - 1.0) / (x + 1.0)),
        [1.0 / 3.0, -2.0 / 2.25, 4.0 / 3.375],
    );
}

#[test]
fn constant_only_macro_and_conditional_callable_report() {
    const C: quest_polynomial::Function<quest_polynomial::typed::Constant> =
        typed_function!(|x| 0.5);
    assert_eq!(C.evaluate(4.0).unwrap(), 0.5);
    let premise = quest_polynomial::ConsistencyAssumption::SameFunctionAndDerivatives;
    let callable = quest_polynomial::CallbackFunction::with_assumed_consistency(
        premise,
        |x| Ok(x.exp()),
        |x| {
            let e = x.exp()?;
            Ok(quest_polynomial::Jet {
                value: e,
                first: e,
                second: e,
            })
        },
    );
    let report = RemezBuilder::new()
        .callable(callable, premise)
        .degree(3)
        .domain(Interval::new(-1.0, 1.0).unwrap())
        .unwrap()
        .run()
        .unwrap();
    assert!(report.conditional_error_bound().upper() < 0.006);
    assert!(matches!(
        report.assumption(),
        quest_polynomial::ConsistencyAssumption::SameFunctionAndDerivatives
    ));
}

#[test]
fn open_generic_callable_has_conditional_ad_and_remez_admission() {
    struct Exponential;
    impl quest_polynomial::GenericCallable for Exponential {
        fn evaluate<B: quest_polynomial::Backend>(
            &self,
            backend: &mut B,
            x: B::Scalar,
        ) -> Result<B::Scalar, B::Error> {
            backend.exp(x)
        }
    }
    let premise = quest_polynomial::ConsistencyAssumption::SameFunctionAndDerivatives;
    let function = quest_polynomial::AssumedFunction::new(Exponential, premise);
    let jet = function
        .jet_backend(&mut quest_polynomial::ScalarBackend::<f64>::new(), 0.5)
        .unwrap();
    assert_eq!(jet.first, 0.5_f64.exp());
    assert_eq!(jet.second, 0.5_f64.exp());
    let result = RemezBuilder::new()
        .callable(function, premise)
        .degree(3)
        .domain(Interval::new(-1.0, 1.0).unwrap())
        .unwrap()
        .run()
        .unwrap();
    assert!(result.conditional_error_bound().upper() < 0.006);
}

#[test]
fn metadata_agrees_after_erasure_and_shared_dynamic_nodes_are_counted_without_traversal() {
    let f = typed_function!(|x| (x.sqrt() + x.ln()) / (x + 1.0));
    assert_eq!(f.metadata(), f.to_dynamic().metadata());
    assert_eq!(f.metadata().positive_jet_arguments, 2);
    assert_eq!(f.metadata().nonzero_denominators, 1);
    let mut expression = quest_polynomial::Expr::variable();
    for _ in 0..70 {
        expression = expression.clone() + expression;
    }
    assert_eq!(
        quest_polynomial::Function::new(expression).metadata().nodes,
        usize::MAX
    );
    assert!(typed_function!(|x| f64::NAN).evaluate(0.0).is_err());
    assert!(typed_function!(|x| x.sqrt()).evaluate(0.0).is_ok());
    assert!(typed_function!(|x| x.sqrt()).jet(0.0).is_err());
}

#[test]
fn legacy_constructor_retains_into_inference() {
    let function = quest_polynomial::Function::new(0.5.into());
    assert_eq!(function.evaluate(2.0).unwrap(), 0.5);
}

#[test]
fn remez_admission_and_repeated_evaluations_respect_expression_work() {
    let domain = Interval::new(-1.0, 1.0).unwrap();
    let mut expression = quest_polynomial::Expr::variable();
    for _ in 0..70 {
        expression = expression.clone() + expression;
    }
    assert!(matches!(
        RemezBuilder::new()
            .target(quest_polynomial::Function::new(expression))
            .domain(domain),
        Err(quest_polynomial::Error::Budget(_))
    ));
    assert!(matches!(
        RemezBuilder::new()
            .target(typed_function!(|x| x.exp()))
            .limits(quest_polynomial::Limits {
                max_work: 1,
                ..Default::default()
            })
            .domain(domain),
        Err(quest_polynomial::Error::Budget(_))
    ));
    // QR alone needs 5^3=125 units; the remaining budget cannot cover the
    // target/polynomial evaluations. This must fail as a budget, not convergence.
    let options = quest_polynomial::RemezOptions {
        degree: 3,
        max_iterations: 1,
        limits: quest_polynomial::Limits {
            max_work: 130,
            ..Default::default()
        },
        ..Default::default()
    };
    assert!(matches!(
        RemezBuilder::new()
            .target(typed_function!(|x| x.exp()))
            .options(options)
            .domain(domain)
            .unwrap()
            .run(),
        Err(quest_polynomial::Error::Budget(_))
    ));
}

#[test]
fn reported_native_failures_retain_original_requests_and_premises() {
    let domain = Interval::new(-1.0, 1.0).unwrap();
    let constant = 0.1_f64;
    let target = typed_function!(|x| x * 0.25 + constant);
    let metadata = target.metadata();
    let failure = RemezBuilder::new()
        .target(target)
        .degree(3)
        .domain(domain)
        .unwrap()
        .limits(quest_polynomial::Limits {
            max_work: 0,
            ..Default::default()
        })
        .run_reported()
        .unwrap_err();
    assert!(matches!(
        failure.error(),
        quest_polynomial::Error::Budget(_)
    ));
    assert_eq!(
        failure.target().evaluate(0.0).unwrap().to_bits(),
        constant.to_bits()
    );
    assert_eq!(failure.target().metadata(), metadata);
    assert_eq!(failure.options().degree, 3);
    let failure = RemezBuilder::new()
        .static_degree(StaticDegree::<3>::new())
        .target(typed_function!(|x| x * 0.25 + constant))
        .domain(domain)
        .unwrap()
        .limits(quest_polynomial::Limits {
            max_work: 0,
            ..Default::default()
        })
        .run_reported()
        .unwrap_err();
    assert_eq!(
        failure.target().evaluate(0.0).unwrap().to_bits(),
        constant.to_bits()
    );
    assert_eq!(failure.options().degree, 3);
    let numerical = RemezBuilder::new()
        .target(typed_function!(|x| x.exp() + constant))
        .options(quest_polynomial::RemezOptions {
            degree: 0,
            max_iterations: 1,
            tolerance: f64::MIN_POSITIVE,
            ..Default::default()
        })
        .domain(domain)
        .unwrap()
        .run_reported()
        .unwrap_err();
    assert!(matches!(
        numerical.error(),
        quest_polynomial::Error::NotEstablished(_)
    ));
    assert_eq!(numerical.target().metadata().nodes, 4);
    let premise = quest_polynomial::ConsistencyAssumption::SameFunctionAndDerivatives;
    let callback = quest_polynomial::CallbackFunction::with_assumed_consistency(
        premise,
        |x| Ok(x.exp()),
        |x| {
            let value = x.exp()?;
            Ok(quest_polynomial::Jet {
                value,
                first: value,
                second: value,
            })
        },
    );
    let Err(failure) = RemezBuilder::new()
        .callable(callback, premise)
        .domain(domain)
        .unwrap()
        .limits(quest_polynomial::Limits {
            max_work: 0,
            ..Default::default()
        })
        .run_reported()
    else {
        panic!("expected retained conditional failure");
    };
    assert!(matches!(
        failure.assumption(),
        quest_polynomial::ConsistencyAssumption::SameFunctionAndDerivatives
    ));
    assert_eq!(failure.target().evaluate(0.0).unwrap(), 1.0);
}
