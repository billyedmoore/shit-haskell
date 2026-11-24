{-# LANGUAGE PolyKinds,TypeFamilies,StandaloneKindSignatures,TypeAbstractions,NoTypeApplications #-}
import Data.Kind (Type, Constraint)
type C :: Type -> Constraint
class C a where
  type F @k a
main :: IO ()
main = pure ()
