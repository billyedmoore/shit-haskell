{-# LANGUAGE ConstrainedClassMethods, ConstraintKinds, InstanceSigs, TypeFamilies #-}
import Data.Kind (Constraint)

type family Needs a :: Constraint
type instance Needs Bool = Eq Bool
class C a where
  method :: Needs b => a -> b -> Bool
instance C Int where
  method :: Needs c => Int -> c -> Bool
  method _ _ = True

main :: IO ()
main = print (method (0 :: Int) False)
