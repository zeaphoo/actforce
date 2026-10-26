# Finance 命令 - 金融数据查询

查询股票、加密货币等金融数据。

## 数据源

- Twelve Data（股票）
- CoinGecko（加密货币）

## 子命令

### stock - 查询股票价格

```bash
thecli finance stock <股票代码>
```

示例：
```bash
# 查询苹果股票
thecli finance stock AAPL

# 查询特斯拉股票
thecli finance stock TSLA

# 查询上证指数
thecli finance stock 000001
```

### crypto - 查询加密货币价格

```bash
thecli finance crypto <币种>
```

示例：
```bash
# 查询比特币
thecli finance crypto bitcoin

# 查询以太坊
thecli finance crypto ethereum

# 使用简称
thecli finance crypto btc
thecli finance crypto eth
```

## 输出信息

- 当前价格
- 24小时涨跌幅
- 市值
- 交易量

## 注意事项

- 股票数据可能有延迟
- 加密货币数据实时更新
