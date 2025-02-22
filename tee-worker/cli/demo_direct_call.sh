#!/bin/bash

# Executes a direct call on a worker and checks the balance afterwards.
#
# setup:
# run all on localhost:
#   litentry-node purge-chain --dev
#   litentry-node --tmp --dev -lruntime=debug
#   rm light_client_db.bin
#   export RUST_LOG=litentry_worker=info,ita_stf=debug
#   litentry-worker init_shard
#   litentry-worker shielding-key
#   litentry-worker signing-key
#   litentry-worker run
#
# then run this script

# usage:
#  demo_direct_call.sh -p <NODEPORT> -P <WORKERPORT> -t <TEST_BALANCE_RUN> -m file
#
# TEST_BALANCE_RUN is either "first" or "second"
# if -m file is set, the mrenclave will be read from file

while getopts ":m:p:P:t:u:V:C:" opt; do
    case $opt in
        t)
            TEST=$OPTARG
            ;;
        m)
            READMRENCLAVE=$OPTARG
            ;;
        p)
            NPORT=$OPTARG
            ;;
        P)
            WORKER1PORT=$OPTARG
            ;;
        u)
            NODEURL=$OPTARG
            ;;
        V)
            WORKER1URL=$OPTARG
            ;;
        C)
            CLIENT_BIN=$OPTARG
            ;;
    esac
done

# Using default port if none given as arguments.
NPORT=${NPORT:-9944}
NODEURL=${NODEURL:-"ws://127.0.0.1"}

WORKER1PORT=${WORKER1PORT:-2000}
WORKER1URL=${WORKER1URL:-"wss://127.0.0.1"}

CLIENT_BIN=${CLIENT_BIN:-"./../bin/litentry-cli"}

echo "Using client binary ${CLIENT_BIN}"
echo "Using node uri ${NODEURL}:${NPORT}"
echo "Using trusted-worker uri ${WORKER1URL}:${WORKER1PORT}"
echo ""

AMOUNTSHIELD=50000000000
AMOUNTTRANSFER=40000000000

CLIENT="${CLIENT_BIN} -p ${NPORT} -P ${WORKER1PORT} -u ${NODEURL} -U ${WORKER1URL}"
read -r MRENCLAVE <<< "$($CLIENT list-workers | awk '/  MRENCLAVE: / { print $2; exit }')"

echo ""
echo "* Create a new incognito account for Alice"
ICGACCOUNTALICE=//AliceIncognito
ICGACCOUNTALICE_PUBKEY=0x50503350955afe8a107d6f115dc253eb5d75a3fe37a90b373db26cc12e3c6661
echo "  Alice's incognito account = ${ICGACCOUNTALICE}"
echo ""

echo "* Create a new incognito account for Bob"
ICGACCOUNTBOB=//BobIncognito
ICGACCOUNTBOB_PUBKEY=0xc24c5b3969d8ec4ca8a655a98dcc136d5d4c29d1206ffe7721e80ebdfa1d0b77
echo "  Bob's incognito account = ${ICGACCOUNTBOB}"
echo ""

echo "* Issue ${AMOUNTSHIELD} tokens to Alice's incognito account"
${CLIENT} trusted --mrenclave ${MRENCLAVE} --direct set-balance ${ICGACCOUNTALICE} ${AMOUNTSHIELD}
echo ""

echo "Get balance of Alice's incognito account"
${CLIENT} trusted --mrenclave ${MRENCLAVE} balance ${ICGACCOUNTALICE}
echo ""

# Send funds from Alice to Bob's account.
echo "* Send ${AMOUNTTRANSFER} funds from Alice's incognito account to Bob's incognito account"
$CLIENT trusted --mrenclave ${MRENCLAVE} --direct transfer ${ICGACCOUNTALICE} ${ICGACCOUNTBOB} ${AMOUNTTRANSFER}
echo ""

# Prevent getter being executed too early and returning an outdated result, before the transfer was made.
echo "* Waiting 6 seconds"
sleep 6
echo ""

echo "* Get balance of Alice's incognito account"
# RESULT=$(${CLIENT} trusted --mrenclave ${MRENCLAVE} balance ${ICGACCOUNTALICE} | xargs)
RESULT=$(${CLIENT} trusted --mrenclave ${MRENCLAVE} get-storage System Account ${ICGACCOUNTALICE_PUBKEY} | jq ".data.free" | xargs)
echo $RESULT
echo ""

echo "* Bob's incognito account balance"
# RESULT=$(${CLIENT} trusted --mrenclave ${MRENCLAVE} balance ${ICGACCOUNTBOB} | xargs)
RESULT=$(${CLIENT} trusted --mrenclave ${MRENCLAVE} get-storage System Account ${ICGACCOUNTBOB_PUBKEY} | jq ".data.free" | xargs)
echo $RESULT
echo ""


# The following tests are for automated CI.
# They only work if you're running from fresh genesis.
case $TEST in
    first)
        if [ "40000000000" = "$RESULT" ]; then
            echo "test passed (1st time)"
            echo ""
            exit 0
        else
            echo "test ran through but balance is wrong. have you run the script from fresh genesis?"
            exit 1
        fi
        ;;
    second)
        if [ "80000000000" = "$RESULT" ]; then
            echo "test passed (2nd time)"
            echo ""
            exit 0
        else
            echo "test ran through but balance is wrong. is this really the second time you run this since genesis?"
            exit 1
        fi
        ;;
esac

exit 0
