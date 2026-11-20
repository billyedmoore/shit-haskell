-- RebindableSyntax: literals, literal patterns, negation and `if` use the names in scope
-- (basement's own `Integral` class with `fromInteger`, and its `ifThenElse`).
{-# LANGUAGE RebindableSyntax #-}
-- (RebindableSyntax implies NoImplicitPrelude: the imports below are all there is.)
import Prelude hiding (fromInteger, negate, fromRational)
import qualified Prelude as P

newtype N = N P.Integer deriving (P.Show, P.Eq)

class Lit a where
  fromInteger :: P.Integer -> a

instance Lit N where
  fromInteger n = N (n P.* 10)

instance Lit P.Int where
  fromInteger = P.fromInteger

instance Lit P.Integer where
  fromInteger n = n

instance Lit P.Double where
  fromInteger = P.fromInteger

fromRational :: P.Rational -> P.Double
fromRational r = P.fromRational r P.* 2

negate :: N -> N
negate (N n) = N (P.negate n P.- 1)

ifThenElse :: P.Bool -> a -> a -> a
ifThenElse c t e = case c of { P.True -> e; P.False -> t }

describe :: N -> P.String
describe 0 = "zero"
describe 30 = "thirty"
describe _ = "other"

count :: P.Int -> P.Int
count 0 = 0
count n = 1 P.+ count (n P.- 1)

main :: IO ()
main = do
  print (3 :: N)
  print (-2 :: N)
  print (1.5 :: P.Double)
  putStrLn (if P.True then "then" else "else")
  putStrLn (describe (N 0) P.++ " " P.++ describe 3 P.++ " " P.++ describe (N 7))
  print (count 4)
