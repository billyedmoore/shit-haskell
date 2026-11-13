-- DerivingVia for a constructor class other than the Functor family (Contravariant).
{-# LANGUAGE DerivingVia #-}
import Data.Functor.Contravariant (Contravariant (..), Op (..), Predicate (..))
import Data.Monoid (All (..))

newtype P a = P { getP :: a -> Bool }
  deriving (Semigroup, Monoid) via a -> All
  deriving Contravariant via Op Bool

main :: IO ()
main = do
  let p = contramap length (P even)
  print (getP p "ab", getP (p <> P (const False)) "ab")
  print (getPredicate (contramap fst (Predicate odd)) (3 :: Int, "x"))
