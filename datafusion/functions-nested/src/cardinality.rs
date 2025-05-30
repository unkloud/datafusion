// Licensed to the Apache Software Foundation (ASF) under one
// or more contributor license agreements.  See the NOTICE file
// distributed with this work for additional information
// regarding copyright ownership.  The ASF licenses this file
// to you under the Apache License, Version 2.0 (the
// "License"); you may not use this file except in compliance
// with the License.  You may obtain a copy of the License at
//
//   http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing,
// software distributed under the License is distributed on an
// "AS IS" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY
// KIND, either express or implied.  See the License for the
// specific language governing permissions and limitations
// under the License.

//! [`ScalarUDFImpl`] definitions for cardinality function.

use crate::utils::make_scalar_function;
use arrow::array::{
    Array, ArrayRef, GenericListArray, MapArray, OffsetSizeTrait, UInt64Array,
};
use arrow::datatypes::{
    DataType,
    DataType::{LargeList, List, Map, Null, UInt64},
};
use datafusion_common::cast::{as_large_list_array, as_list_array, as_map_array};
use datafusion_common::exec_err;
use datafusion_common::utils::{take_function_args, ListCoercion};
use datafusion_common::Result;
use datafusion_expr::{
    ArrayFunctionArgument, ArrayFunctionSignature, ColumnarValue, Documentation,
    ScalarUDFImpl, Signature, TypeSignature, Volatility,
};
use datafusion_macros::user_doc;
use std::any::Any;
use std::sync::Arc;

make_udf_expr_and_func!(
    Cardinality,
    cardinality,
    array,
    "returns the total number of elements in the array or map.",
    cardinality_udf
);

impl Cardinality {
    pub fn new() -> Self {
        Self {
            signature: Signature::one_of(
                vec![
                    TypeSignature::ArraySignature(ArrayFunctionSignature::Array {
                        arguments: vec![ArrayFunctionArgument::Array],
                        array_coercion: Some(ListCoercion::FixedSizedListToList),
                    }),
                    TypeSignature::ArraySignature(ArrayFunctionSignature::MapArray),
                ],
                Volatility::Immutable,
            ),
            aliases: vec![],
        }
    }
}

#[user_doc(
    doc_section(label = "Array Functions"),
    description = "Returns the total number of elements in the array.",
    syntax_example = "cardinality(array)",
    sql_example = r#"```sql
> select cardinality([[1, 2, 3, 4], [5, 6, 7, 8]]);
+--------------------------------------+
| cardinality(List([1,2,3,4,5,6,7,8])) |
+--------------------------------------+
| 8                                    |
+--------------------------------------+
```"#,
    argument(
        name = "array",
        description = "Array expression. Can be a constant, column, or function, and any combination of array operators."
    )
)]
#[derive(Debug)]
pub struct Cardinality {
    signature: Signature,
    aliases: Vec<String>,
}

impl Default for Cardinality {
    fn default() -> Self {
        Self::new()
    }
}
impl ScalarUDFImpl for Cardinality {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn name(&self) -> &str {
        "cardinality"
    }

    fn signature(&self) -> &Signature {
        &self.signature
    }

    fn return_type(&self, _arg_types: &[DataType]) -> Result<DataType> {
        Ok(UInt64)
    }

    fn invoke_with_args(
        &self,
        args: datafusion_expr::ScalarFunctionArgs,
    ) -> Result<ColumnarValue> {
        make_scalar_function(cardinality_inner)(&args.args)
    }

    fn aliases(&self) -> &[String] {
        &self.aliases
    }

    fn documentation(&self) -> Option<&Documentation> {
        self.doc()
    }
}

/// Cardinality SQL function
pub fn cardinality_inner(args: &[ArrayRef]) -> Result<ArrayRef> {
    let [array] = take_function_args("cardinality", args)?;
    match array.data_type() {
        Null => Ok(Arc::new(UInt64Array::from_value(0, array.len()))),
        List(_) => {
            let list_array = as_list_array(array)?;
            generic_list_cardinality::<i32>(list_array)
        }
        LargeList(_) => {
            let list_array = as_large_list_array(array)?;
            generic_list_cardinality::<i64>(list_array)
        }
        Map(_, _) => {
            let map_array = as_map_array(array)?;
            generic_map_cardinality(map_array)
        }
        arg_type => {
            exec_err!("cardinality does not support type {arg_type}")
        }
    }
}

fn generic_map_cardinality(array: &MapArray) -> Result<ArrayRef> {
    let result: UInt64Array = array
        .iter()
        .map(|opt_arr| opt_arr.map(|arr| arr.len() as u64))
        .collect();
    Ok(Arc::new(result))
}

fn generic_list_cardinality<O: OffsetSizeTrait>(
    array: &GenericListArray<O>,
) -> Result<ArrayRef> {
    let result = array
        .iter()
        .map(|arr| match crate::utils::compute_array_dims(arr)? {
            Some(vector) => Ok(Some(vector.iter().map(|x| x.unwrap()).product::<u64>())),
            None => Ok(None),
        })
        .collect::<Result<UInt64Array>>()?;
    Ok(Arc::new(result) as ArrayRef)
}
