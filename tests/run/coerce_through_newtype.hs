-- `coerce` of an overloaded function to a newtype's version, its type variable found through the
-- newtype (containers' `sconcat = coerce intersections` for `Intersection`): the inner type's
-- parameter is nominal, so `Set t` and `Set a` coerce only when t is a.
{-# LANGUAGE RoleAnnotations #-}
import Data.Coerce (coerce)
import Data.List.NonEmpty (NonEmpty (..))

data Set a = Set [a] deriving Show
type role Set nominal

newtype Intersection a = Intersection { getIntersection :: Set a } deriving Show

intersections :: Ord a => NonEmpty (Set a) -> Set a
intersections (Set x :| rest) = Set (foldr (\(Set y) acc -> filter (`elem` y) acc) x rest)

sconcatI :: Ord a => NonEmpty (Intersection a) -> Intersection a
sconcatI = coerce intersections

main :: IO ()
main = print (sconcatI (Intersection (Set [1, 2, 3 :: Int]) :| [Intersection (Set [2, 3, 4]), Intersection (Set [3, 2])]))
