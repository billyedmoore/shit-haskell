{-# LANGUAGE ConstraintKinds, InstanceSigs, ScopedTypeVariables, TypeFamilies #-}
import Data.Kind (Constraint)

type family Needs a :: Constraint
class C a where
  method :: a -> Bool
instance forall a. C [a] where
  method :: Needs a => [a] -> Bool
  method _ = True

main :: IO ()
main = pure ()
