use std::{
    collections::HashMap,
    fmt::Debug,
    fs::File,
    io::{self, BufRead, BufReader, Write},
    path::PathBuf,
};

use anyhow::Error;
use clap::Parser;
use regex::Regex;
use term_size;

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    file: PathBuf,
    ///FPGA part name
    name: Option<String>,
}

enum States {
    SeekTable,
    ReadHeader,
    ReadTable,
    END,
}

struct Record {
    fields: HashMap<String, String>,
}

impl Record {
    fn new(headers: &[String], values: &[&str]) -> Self {
        let mut fields = HashMap::new();
        for (header, value) in headers.iter().zip(values.iter()) {
            fields.insert(header.clone(), value.to_string());
        }
        Record { fields }
    }
}

impl Debug for Record {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // 获取所有键并排序
        let mut keys: Vec<&String> = self.fields.keys().collect();
        keys.sort();
        for key in keys {
            let value = self.fields.get(key).unwrap();
            write!(f, "{:?}: {:?} ", key, value)?;
        }
        Ok(())
    }
}

fn main() -> Result<(), Error> {
    let args = Args::parse();

    let re_blank = Regex::new(r"^\s*$").unwrap();
    let re_spilt_header = Regex::new(r"\s{2,}").unwrap();

    let mut index = 0;
    let mut pins_count: usize = 0;
    let mut state = States::SeekTable;
    let mut headers: Vec<String> = Vec::new();
    let mut records: Vec<Record> = Vec::new();

    let file = File::open(args.file)?;
    let mut buf_reader = BufReader::new(file);

    let mut lines_iter = buf_reader.lines().map(|l| l.unwrap()).enumerate();

    while let Some((line_num, line)) = lines_iter.next() {
        match state {
            States::SeekTable => {
                if re_blank.is_match(&line) {
                    println!("{}", "-".repeat(term_size::dimensions().unwrap().0));
                    index = line_num;
                    state = States::ReadHeader;
                }
            }
            States::ReadHeader => {
                // 解析表头
                headers = re_spilt_header
                    .split(&line.trim())
                    .map(|s| s.to_string())
                    .collect();
                state = States::ReadTable
            }
            States::ReadTable => {
                if re_blank.is_match(&line) {
                    println!("{}", "-".repeat(term_size::dimensions().unwrap().0));
                    state = States::END;
                    continue;
                }
                // 逐行解析数据
                let values: Vec<&str> = re_spilt_header.split(line.trim()).collect();
                if values.len() == headers.len() {
                    let record = Record::new(&headers, &values);
                    records.push(record);
                }
            }

            States::END => {
                pins_count = records.len();

                println!("total pins parsed: {}", pins_count);
            }
        }
    }

    println!("\nAvailable fields:");
    for (i, header) in headers.iter().enumerate() {
        println!("{}: {}", i, header);
    }

    print!("Enter the number of the field to group by: ");
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let field_index: usize = input.trim().parse().unwrap();

    if field_index >= headers.len() {
        eprintln!("Invalid field index");
    }

    let group_field = &headers[field_index];

    // 根据用户选择的字段进行分组
    let mut groups: HashMap<String, Vec<Record>> = HashMap::new();

    for record in records {
        let key = record.fields.get(group_field).unwrap().clone();
        groups.entry(key).or_insert_with(Vec::new).push(record);
    }

    // 让用户选择排序字段
    print!("Enter the number of the field to sort by within groups: ");
    io::stdout().flush().unwrap();

    input.clear();
    io::stdin().read_line(&mut input).unwrap();
    let sort_field_index: usize = input.trim().parse().unwrap();

    if sort_field_index >= headers.len() {
        eprintln!("Invalid field index");
    }

    let sort_field = &headers[sort_field_index];

    // 打印分组并排序后的数据
    println!(
        "\nGrouped and sorted data by {} and {}:",
        group_field, sort_field
    );
    for (key, group) in &mut groups {
        println!("Group {}: ", key);
        group.sort_by(|a, b| {
            a.fields
                .get(sort_field)
                .unwrap()
                .cmp(b.fields.get(sort_field).unwrap())
        });
        for record in group {
            println!("{:?}", record);
        }
    }

    // 生成 KiCad 库文件
    let mut kicad_lib = String::new();
    kicad_lib.push_str("EESchema-LIBRARY Version 2.4\n#encoding utf-8\n");

    let mut unit_number = 1;
    kicad_lib.push_str(&format!(
        "DEF {} U 0 40 Y Y {} L N\n",
        args.name.unwrap_or("XilinxFPGA".to_string()),
        groups.len()
    ));
    kicad_lib.push_str(&format!("F0 \"U\" 0 300 50 H V C CNN\n"));
    kicad_lib.push_str(&format!("F1 \"FPGA\" 0 200 50 H V C CNN\n"));
    kicad_lib.push_str(&format!("F2 \"\" 0 0 50 H I C CNN\n"));
    kicad_lib.push_str(&format!("F3 \"\" 0 0 50 H I C CNN\n"));
    kicad_lib.push_str("DRAW\n");

    for (_key, group) in groups.iter() {
        kicad_lib.push_str(&format!(
            "S 150 150 2850 -{} {} 1 0 f\n",
            group.len() / 2 * 100 + 50,
            unit_number
        ));
        for (i, record) in group.iter().enumerate() {
            let pin = record.fields.get("Pin").unwrap();
            let pin_name = record.fields.get("Pin Name").unwrap();
            let posx = if i < group.len() / 2 { 0 } else { 3000 };
            let posy = if i < group.len() / 2 {
                i * 100
            } else {
                (i - group.len() / 2) * 100
            };
            let orientation = if i < group.len() / 2 { "R" } else { "L" };
            kicad_lib.push_str(&format!(
                "X {} {} {} -{} 150 {} 50 50 {} 1 P\n",
                pin_name, pin, posx, posy, orientation, unit_number
            ));
        }

        unit_number += 1;
    }

    kicad_lib.push_str("ENDDRAW\n");
    kicad_lib.push_str("ENDDEF\n");
    kicad_lib.push_str("#\n#End Library\n");

    // 将字符串写入 .lib 文件
    let filename = "output.lib";
    let mut file = File::create(filename)?;
    file.write_all(kicad_lib.as_bytes())?;

    println!("Finished Generation");
    println!("{} pins parsed {} units generated", pins_count, groups.len());

    Ok(())
}
