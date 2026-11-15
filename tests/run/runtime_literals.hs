module Main where
import qualified LiteralCustom as Custom

main :: IO ()
main = do
  print ([7, -7, maxBound, minBound, 9223372036854775808,
          18446744073709551617, -9223372036854775809] :: [Int])
  print (123456789012345678901234567890 :: Integer)
  print ((9007199254740993 :: Double) == 9007199254740992)
  print ((36893488147419107329 :: Double) > 36893488147419103232)
  print (isNegativeZero (-0.0 :: Double), (1e-324 :: Double) == 0, (5e-324 :: Double) > 0)
  print (0.125 :: Double)
  print (7 :: Custom.Int)
