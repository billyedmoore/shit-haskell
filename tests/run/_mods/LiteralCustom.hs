module LiteralCustom (Int(..)) where
import Prelude hiding (Int)

data Int = Int Integer deriving (Eq, Show)

instance Num Int where
  Int a + Int b = Int (a + b)
  Int a - Int b = Int (a - b)
  Int a * Int b = Int (a * b)
  negate (Int a) = Int (negate a)
  abs (Int a) = Int (abs a)
  signum (Int a) = Int (signum a)
  fromInteger n = Int (n + 100)
