-- A class with a quantified superclass (Eq1's forall a. Eq a => Eq (f a)): a given Eq1 f gives
-- Eq (f x) (data-fix's Eq (Fix f)).
import Data.Functor.Classes

newtype Fix f = Fix (f (Fix f))

instance Eq1 f => Eq (Fix f) where
    Fix a == Fix b = a == b

instance Ord1 f => Ord (Fix f) where
    compare (Fix a) (Fix b) = compare a b

data ListF a r = Nil | Cons a r deriving (Eq, Ord)

instance Eq a => Eq1 (ListF a) where
  liftEq _ Nil Nil = True
  liftEq eq (Cons a r) (Cons b s) = a == b && eq r s
  liftEq _ _ _ = False

instance Ord a => Ord1 (ListF a) where
  liftCompare _ Nil Nil = EQ
  liftCompare _ Nil _ = LT
  liftCompare _ _ Nil = GT
  liftCompare cmp (Cons a r) (Cons b s) = compare a b <> cmp r s

fromList :: [a] -> Fix (ListF a)
fromList = foldr (\x r -> Fix (Cons x r)) (Fix Nil)

main :: IO ()
main = do
  print (fromList [1, 2, 3 :: Int] == fromList [1, 2, 3], fromList [1, 2 :: Int] == fromList [1, 3])
  print (compare (fromList [1, 2 :: Int]) (fromList [1, 3]), compare (fromList "ab") (fromList "a"))
