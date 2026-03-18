#!/usr/bin/env python3
# 同步原语实验数据分析脚本
# 用于解析实验输出并生成可视化报告

import re
import os
import pandas as pd
import matplotlib
# 在无显示环境下使用Agg后端
matplotlib.use('Agg')
import matplotlib.pyplot as plt
import seaborn as sns
from datetime import datetime

# 配置中文字体支持
def setup_chinese_font():
    """配置matplotlib中文字体支持"""
    import matplotlib.font_manager as fm
    
    # 尝试常见的中文字体
    chinese_fonts = [
        'SimHei',           # 黑体（Windows/Linux）
        'Microsoft YaHei',  # 微软雅黑（Windows）
        'PingFang SC',      # 苹方（Mac）
        'Noto Sans CJK SC', # 思源黑体（Linux）
        'WenQuanYi Micro Hei',  # 文泉驿微米黑（Linux）
        'DejaVu Sans',      # 备用字体
    ]
    
    # 查找系统中可用的中文字体
    available_fonts = [f.name for f in fm.fontManager.ttflist]
    
    selected_font = None
    for font in chinese_fonts:
        if font in available_fonts:
            selected_font = font
            break
    
    if selected_font:
        plt.rcParams['font.sans-serif'] = [selected_font]
        plt.rcParams['axes.unicode_minus'] = False  # 解决负号显示问题
        print(f"使用中文字体: {selected_font}")
    else:
        # 如果没有找到中文字体，使用英文标签
        print("警告: 未找到中文字体，将使用英文标签")
        return False
    
    return True

# 设置中文字体
HAS_CHINESE_FONT = setup_chinese_font()

def parse_experiment_output(output_file):
    """解析实验输出文件，提取CSV格式数据"""
    
    data = []
    
    with open(output_file, 'r', encoding='utf-8') as f:
        for line in f:
            # 匹配CSV数据行
            if line.startswith('EXPERIMENT_CSV,'):
                parts = line.strip().split(',')
                if len(parts) == 6:  # 确保数据格式正确
                    data.append({
                        'lock_type': parts[1],
                        'thread_count': int(parts[2]),
                        'total_time_ms': int(parts[3]),
                        'total_operations': int(parts[4]),
                        'throughput_ops_per_sec': float(parts[5])
                    })
    
    return pd.DataFrame(data)

def generate_performance_report(df):
    """生成性能对比报告"""
    
    print("=== 同步原语性能对比报告 ===")
    print(f"生成时间: {datetime.now().strftime('%Y-%m-%d %H:%M:%S')}")
    print(f"数据总量: {len(df)} 条记录")
    print()
    
    # 按锁类型分组统计
    lock_stats = df.groupby('lock_type').agg({
        'throughput_ops_per_sec': ['mean', 'std', 'min', 'max'],
        'total_time_ms': 'mean',
        'total_operations': 'sum'
    }).round(2)
    
    print("各锁类型性能统计:")
    print(lock_stats)
    print()
    
    # 计算性能提升百分比
    spinlock_avg = df[df['lock_type'] == 'spinlock']['throughput_ops_per_sec'].mean()
    mutex_avg = df[df['lock_type'] == 'mutex']['throughput_ops_per_sec'].mean()
    
    if mutex_avg > 0:
        improvement = ((spinlock_avg - mutex_avg) / mutex_avg) * 100
        print(f"自旋锁相比互斥锁性能提升: {improvement:.1f}%")
    
    return lock_stats

def create_visualizations(df, output_dir='results'):
    """创建可视化图表"""
    
    import os
    os.makedirs(output_dir, exist_ok=True)
    
    # 设置图表样式
    plt.style.use('seaborn-v0_8')
    sns.set_palette("husl")
    
    # 根据是否有中文字体选择标签语言
    if HAS_CHINESE_FONT:
        title_1 = '不同线程数下的锁吞吐量对比'
        title_2 = '不同线程数下的执行时间对比'
        title_3 = '自旋锁性能 vs 线程数'
        title_4 = '互斥锁性能 vs 线程数'
        title_5 = '执行时间对比'
        title_6 = '自旋锁/互斥锁性能比率'
        xlabel = '线程数量'
        ylabel_throughput = '吞吐量 (操作/秒)'
        ylabel_time = '总执行时间 (毫秒)'
        ylabel_ratio = '性能比率'
        legend_title = '锁类型'
    else:
        title_1 = 'Lock Throughput Comparison by Thread Count'
        title_2 = 'Execution Time Comparison by Thread Count'
        title_3 = 'Spinlock Performance vs Thread Count'
        title_4 = 'Mutex Performance vs Thread Count'
        title_5 = 'Execution Time Comparison'
        title_6 = 'Spinlock/Mutex Performance Ratio'
        xlabel = 'Thread Count'
        ylabel_throughput = 'Throughput (ops/s)'
        ylabel_time = 'Total Execution Time (ms)'
        ylabel_ratio = 'Performance Ratio'
        legend_title = 'Lock Type'
    
    # 1. 吞吐量对比图
    plt.figure(figsize=(10, 6))
    sns.barplot(data=df, x='thread_count', y='throughput_ops_per_sec', hue='lock_type')
    plt.title(title_1)
    plt.xlabel(xlabel)
    plt.ylabel(ylabel_throughput)
    plt.legend(title=legend_title)
    plt.tight_layout()
    plt.savefig(f'{output_dir}/throughput_comparison.png', dpi=300, bbox_inches='tight')
    plt.close()
    
    # 2. 执行时间对比图
    plt.figure(figsize=(10, 6))
    sns.lineplot(data=df, x='thread_count', y='total_time_ms', hue='lock_type', marker='o')
    plt.title(title_2)
    plt.xlabel(xlabel)
    plt.ylabel(ylabel_time)
    plt.legend(title=legend_title)
    plt.tight_layout()
    plt.savefig(f'{output_dir}/execution_time.png', dpi=300, bbox_inches='tight')
    plt.close()
    
    # 3. 线程数对性能的影响
    plt.figure(figsize=(12, 8))
    
    plt.subplot(2, 2, 1)
    spinlock_data = df[df['lock_type'] == 'spinlock']
    plt.plot(spinlock_data['thread_count'], spinlock_data['throughput_ops_per_sec'], 'o-', label='Spinlock')
    plt.title(title_3)
    plt.xlabel(xlabel)
    plt.ylabel(ylabel_throughput)
    
    plt.subplot(2, 2, 2)
    mutex_data = df[df['lock_type'] == 'mutex']
    plt.plot(mutex_data['thread_count'], mutex_data['throughput_ops_per_sec'], 'o-', label='Mutex')
    plt.title(title_4)
    plt.xlabel(xlabel)
    plt.ylabel(ylabel_throughput)
    
    plt.subplot(2, 2, 3)
    plt.plot(spinlock_data['thread_count'], spinlock_data['total_time_ms'], 'o-', label='Spinlock')
    plt.plot(mutex_data['thread_count'], mutex_data['total_time_ms'], 'o-', label='Mutex')
    plt.title(title_5)
    plt.xlabel(xlabel)
    plt.ylabel(ylabel_time)
    plt.legend()
    
    plt.subplot(2, 2, 4)
    # 计算性能比率
    performance_ratio = []
    for threads in sorted(df['thread_count'].unique()):
        spin_throughput = df[(df['lock_type'] == 'spinlock') & (df['thread_count'] == threads)]['throughput_ops_per_sec'].mean()
        mutex_throughput = df[(df['lock_type'] == 'mutex') & (df['thread_count'] == threads)]['throughput_ops_per_sec'].mean()
        if mutex_throughput > 0:
            ratio = spin_throughput / mutex_throughput
            performance_ratio.append((threads, ratio))
    
    threads, ratios = zip(*performance_ratio)
    plt.plot(threads, ratios, 'o-', color='red')
    plt.axhline(y=1, color='gray', linestyle='--', alpha=0.5)
    plt.title(title_6)
    plt.xlabel(xlabel)
    plt.ylabel(ylabel_ratio)
    
    plt.tight_layout()
    plt.savefig(f'{output_dir}/detailed_analysis.png', dpi=300, bbox_inches='tight')
    plt.close()
    
    print(f"图表已保存到 {output_dir}/ 目录")

def generate_csv_report(df, output_file='experiment_results.csv'):
    """生成CSV格式的详细报告"""
    
    # 添加详细分析列
    df['operations_per_thread'] = df['total_operations'] / df['thread_count']
    df['time_per_operation_ms'] = df['total_time_ms'] / df['total_operations']
    
    # 保存为CSV
    df.to_csv(output_file, index=False, encoding='utf-8')
    print(f"详细数据已保存到 {output_file}")
    
    return df

def main():
    """主函数"""
    
    import sys
    
    # 支持命令行参数或默认文件
    if len(sys.argv) > 1:
        input_file = sys.argv[1]
    else:
        # 默认查找最新的结果文件
        import glob
        result_files = glob.glob('results/*.txt')
        if result_files:
            # 按修改时间排序，取最新的
            result_files.sort(key=lambda x: os.path.getmtime(x), reverse=True)
            input_file = result_files[0]
        else:
            input_file = 'experiment_output.txt'
    
    print(f"使用输入文件: {input_file}")
    
    try:
        # 解析数据
        print("正在解析实验数据...")
        df = parse_experiment_output(input_file)
        
        if df.empty:
            print("未找到有效的实验数据，请检查输入文件格式")
            return
        
        print(f"成功解析 {len(df)} 条实验记录")
        
        # 生成报告
        stats = generate_performance_report(df)
        
        # 创建可视化
        print("\n正在生成可视化图表...")
        create_visualizations(df)
        
        # 生成CSV报告
        detailed_df = generate_csv_report(df)
        
        print("\n=== 分析完成 ===")
        print("生成的文件:")
        print("- results/throughput_comparison.png: 吞吐量对比图")
        print("- results/execution_time.png: 执行时间对比图")
        print("- results/detailed_analysis.png: 详细分析图")
        print("- experiment_results.csv: 详细数据表格")
        
    except FileNotFoundError:
        print(f"错误: 找不到输入文件 {input_file}")
        print("请先运行实验并将输出重定向到该文件")
        print("示例: cargo run 2>&1 | tee experiment_output.txt")
    except Exception as e:
        print(f"分析过程中出现错误: {e}")

if __name__ == "__main__":
    main()