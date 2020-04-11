package main

import (
	"bufio"
	"fmt"
	"github.com/ziutek/rrd"
	"math"
	"os"
	"path"
	"strconv"
	"strings"
	"time"
)

func plot(nodesFile, dataDir, content, node, output string, window, duration uint, endTime string) {
	nodes := make(map[string]string)
	file, err := os.Open(nodesFile)
	if err != nil {
		fmt.Println("Error opening node list:", err)
		os.Exit(1)
	}
	defer file.Close()
	scanner := bufio.NewScanner(file)
	for scanner.Scan() {
		s := strings.Split(scanner.Text(), ",")
		name := s[0]
		p := path.Clean(dataDir + "/" + name + ".rrd")
		nodes[name] = p
	}
	if err := scanner.Err(); err != nil {
		fmt.Println("Error reading node list:", err)
		os.Exit(1)
	}
	if _, ok := nodes[node]; !ok {
		fmt.Println("Node", node, "does not exist")
		os.Exit(1)
	}
	xporter := rrd.NewExporter()
	g := rrd.NewGrapher()
	g.SetSize(800, 300)
	switch content {
	case "txrate":
		// create def for each node, and create a cdef to sum them up (for generate) and get max/min (for confirm)
		genSum := ""
		nodeConfirmSet := ""
		trend_cmd := fmt.Sprintf(",%v,TRENDNAN", window)
		for n, p := range nodes {
			g.Def(n+"_gen", p, "generated_tx", "AVERAGE")
			if genSum == "" {
				genSum = n + "_gen"
			} else {
				genSum += "," + n + "_gen,+"
			}
			g.Def(n+"_confirm", p, "confirmed_tx", "AVERAGE")
			xporter.Def(n+"_confirm", p, "confirmed_tx", "AVERAGE")
			if window != 1 {
				// if we are doing windowed average, we will be computing the min, max, avg of the
				// windowed average value
				g.CDef(n+"_confirm_wa", n+"_confirm"+trend_cmd)
				xporter.CDef(n+"_confirm_wa", n+"_confirm"+trend_cmd)
				nodeConfirmSet += n + "_confirm_wa,"
			} else {
				nodeConfirmSet += n + "_confirm,"
			}
		}
		g.Def(node+"_tx_blk_confirm", nodes[node], "confirmed_tx_blk", "AVERAGE")
		g.CDef("gen_sum", genSum)
		g.CDef("confirm_max", fmt.Sprintf("%s%v,SMAX", nodeConfirmSet, len(nodes)))
		g.CDef("confirm_min", fmt.Sprintf("%s%v,SMIN", nodeConfirmSet, len(nodes)))
		g.CDef("confirm_avg", fmt.Sprintf("%s%v,AVG", nodeConfirmSet, len(nodes)))
		g.CDef("min_max_diff", "confirm_max,confirm_min,-")
		xporter.CDef("confirm_avg", fmt.Sprintf("%s%v,AVG", nodeConfirmSet, len(nodes)))
		xporter.XportDef("confirm_avg", "avg")

		// enable sliding window if necessary
		if window != 1 {
			g.CDef("gen_sum_wa", "gen_sum"+trend_cmd)
			//g.Line(1.0, "gen_sum_wa", "00FF00", "Total Generated")
			g.Line(1.0, node+"_confirm_wa", "FF0000", node+" Confirmed")
		} else {
			//g.Line(1.0, "gen_sum", "00FF00", "Total Generated")
			g.Line(1.0, node+"_confirm", "FF0000", node+" Confirmed")
		}
		g.Line(1.0, "confirm_min", "")
		g.Area("min_max_diff", "0000FF20", "STACK") // this area is stacked on confirm_min, so we should sub min
		g.Line(1.0, "confirm_avg", "0000FF", "Avg Confirmed")
		g.Tick(node+"_tx_blk_confirm", "00FFFF20", "1.0", "Tx Block Confirmation")
		g.SetVLabel("TPS")
		g.SetTitle("EVM Throughput")
	case "blockdelay":
		g.Def(node+"_proposer_delay", nodes[node], "proposer_delay_mean", "AVERAGE", fmt.Sprintf("step=%v", window))
		g.Def(node+"_voter_delay", nodes[node], "voter_delay_mean", "AVERAGE", fmt.Sprintf("step=%v", window))
		g.Def(node+"_tx_delay", nodes[node], "tx_delay_mean", "AVERAGE", fmt.Sprintf("step=%v", window))
		g.Line(1.0, node+"_proposer_delay", "FF0000", "Proposer")
		g.Line(1.0, node+"_voter_delay", "00FF00", "Voter")
		g.Line(1.0, node+"_tx_delay", "0000FF", "Tx")
		g.SetVLabel("Latency (ms)")
		g.SetTitle("Block Latency (" + node + ")")
	case "confirm":
		nodeConfirmSet := ""
		trend_cmd := fmt.Sprintf(",%v,TRENDNAN", window)
		//g.Def(node+"_txblk_cfm", nodes[node], "txblk_cfm_mean", "AVERAGE")
		//g.CDef(node+"_txblk_cfm_wa", node+"_txblk_cfm"+trend_cmd)
		for n, p := range nodes {
			g.Def(n+"_txblk_cfm", p, "txblk_cfm_mean", "AVERAGE")
			g.CDef(n+"_txblk_cfm_wa", n+"_txblk_cfm"+trend_cmd)
			xporter.Def(n+"_txblk_cfm", p, "txblk_cfm_mean", "AVERAGE")
			nodeConfirmSet += n + "_txblk_cfm,"
		}
		g.CDef("txblk_cfm", fmt.Sprintf("%s%v,AVG", nodeConfirmSet, len(nodes)))
		xporter.CDef("txblk_cfm", fmt.Sprintf("%s%v,AVG", nodeConfirmSet, len(nodes)))
		xporter.XportDef("txblk_cfm", "avg")
		//xporter.CDef(node+"_txblk_cfm_wa", node+"_txblk_cfm"+trend_cmd)
		//xporter.XportDef(node+"_txblk_cfm", "avg")
		g.Line(1.0, "txblk_cfm", "FF0000", "Tx Block (All nodes, Instaneous)")
		g.Line(1.0, node+"_txblk_cfm", "0000FF", "Tx Block (Instaneous)")
		if window != 1 {
			g.Line(1.0, node+"_txblk_cfm_wa", "00FF00", "TX Block (Windowed)")
		}
		g.SetVLabel("Latency (s)")
		g.SetTitle("Confirmation Latency (" + node + ")")
	case "queue":
		g.Def(node+"_queue", nodes[node], "queue_length", "AVERAGE", fmt.Sprintf("step=%v", window))
		g.Line(1.0, node+"_queue", "0000FF")
		g.SetVLabel("Queue Length (Msg)")
		g.SetTitle("Queue Length (" + node + ")")
	case "mining":
		minePropSum := ""
		mineVoterSum := ""
		mineTxSum := ""
		for n, p := range nodes {
			g.Def(n+"_mined_proposer", p, "mined_proposer", "AVERAGE")
			g.Def(n+"_mined_voter", p, "mined_voter", "AVERAGE")
			g.Def(n+"_mined_tx", p, "mined_tx", "AVERAGE")
			if minePropSum == "" {
				minePropSum = n + "_mined_proposer"
			} else {
				minePropSum += "," + n + "_mined_proposer,+"
			}
			if mineVoterSum == "" {
				mineVoterSum = n + "_mined_voter"
			} else {
				mineVoterSum += "," + n + "_mined_voter,+"
			}
			if mineTxSum == "" {
				mineTxSum = n + "_mined_tx"
			} else {
				mineTxSum += "," + n + "_mined_tx,+"
			}
		}
		g.CDef("mined_proposer_sum", minePropSum)
		g.CDef("mined_voter_sum", mineVoterSum)
		g.CDef("mined_tx_sum", mineTxSum)
		//g.Line(1.0, node+"_mined_proposer", "FF0000", node + " Proposer")
		//g.Line(1.0, node+"_mined_voter", "00FF00", node + " Voter")
		//g.Line(1.0, node+"_mined_tx", "0000FF", node + " Tx")
		g.Line(1.0, "mined_proposer_sum", "FF0000", "Total Proposer")
		g.Line(1.0, "mined_voter_sum", "00FF00", "Total Voter")
		g.Line(1.0, "mined_tx_sum", "0000FF", "Total Tx")
		g.Tick("mined_proposer_sum", "808080", "1.0", "Proposer Mined")
		g.SetVLabel("Mining Rate (Blk/s)")
		g.SetTitle("Mining Rate (" + node + ")")
	default:
		fmt.Println("Plot content options: txrate, blockdelay, queue, mining")
		os.Exit(1)
	}
	t_offset, e := strconv.ParseInt(endTime, 10, 64)
	t := time.Now()
	if e == nil {
		t = time.Unix(t_offset, 0) //t.Add(-time.Minute * time.Duration(t_offset))
	}
	_, e = g.SaveGraph(output, t.Add(-time.Duration(duration)*time.Second), t)
	if e != nil {
		fmt.Println("Error plotting data:", e)
	}
	if content == "txrate" || content == "confirm" {
		ignore := false
		xportRes, e := xporter.Xport(t.Add(-time.Duration(duration)*time.Second), t, time.Second)
		if e != nil {
			fmt.Println("Error export data:", e)
		} else {
			defer xportRes.FreeValues()
			row := 0
			cnt := 0
			xport_sum := 0.0
			for ti := xportRes.Start.Add(xportRes.Step); ti.Before(xportRes.End) || ti.Equal(xportRes.End); ti = ti.Add(xportRes.Step) {
				v := xportRes.ValueAt(0, row)
				if math.IsNaN(v) {
					ignore = true
				} else {
					xport_sum += v
					cnt++
				}
				row++
			}
			if ignore {
				fmt.Printf("Ignore: data has NaN\n")
			}
			fmt.Printf("XportResult: Avg %s in %d seconds: %v\n", content, duration, math.Round(xport_sum/float64(cnt)))
		}
	}
}
