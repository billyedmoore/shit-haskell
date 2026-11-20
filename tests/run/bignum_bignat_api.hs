-- ghc-bignum's BigNat API (cborg's bigNatSubWord#, integer-gmp).
{-# LANGUAGE MagicHash, UnboxedTuples, UnboxedSums #-}
import GHC.Num.BigNat
import GHC.Num.Integer
import GHC.Exts

big :: Integer
big = 2 ^ (100 :: Int) + 12345

main :: IO ()
main = case big of
  IP b -> run b
  _ -> putStrLn "small"

run :: BigNat# -> IO ()
run bn = do
  print (integerFromBigNat# (bigNatAddWord# bn 5##), integerFromBigNat# (bigNatMulWord# bn 3##))
  print (W# (bigNatPopCount# bn), I# (bigNatTestBit# bn 100##), I# (bigNatTestBit# bn 99##))
  print (integerFromBigNat# (bigNatShiftR# bn 90##), integerFromBigNat# (bigNatShiftL# bn 3##))
  print (bigNatSizeInBase 10 bn, W# (bigNatSizeInBase# 256## bn), W# (bigNatRemWord# bn 1000##))
  case bigNatSubWord# bn 12345## of
    (# | r #) -> print (integerFromBigNat# r)
    (# _ | #) -> putStrLn "negative"
  case bigNatSub (bigNatFromWord# 3##) bn of
    (# | r #) -> print (integerFromBigNat# r)
    (# _ | #) -> putStrLn "negative"
  case bigNatQuotRemWord# bn 7## of (# q, r #) -> print (integerFromBigNat# q, W# r)
  print (integerFromBigNat# (bigNatFromWord2# 1## 2##), bigNatCompareWord# bn 5##, I# (bigNatGtWord# bn 5##))
  print (integerFromBigNat# (bigNatGcd bn (bigNatFromWord# 15##)), integerFromBigNat# (bigNatSqr (bigNatFromWord# 12##)))
