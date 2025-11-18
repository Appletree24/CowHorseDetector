use std::collections::HashMap;

use anyhow::{bail, Result};

pub fn parse_aliases(raw: &[String]) -> Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    for entry in raw {
        let parts: Vec<_> = entry.splitn(2, '=').collect();
        if parts.len() != 2 {
            bail!("别名参数格式应为 旧名=统一名，当前为：{entry}");
        }
        let from = parts[0].trim();
        let to = parts[1].trim();
        if from.is_empty() || to.is_empty() {
            bail!("别名参数不能为空：{entry}");
        }
        map.insert(from.to_string(), to.to_string());
    }
    Ok(map)
}
