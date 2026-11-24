{-# LANGUAGE PolyKinds,TypeFamilies,StandaloneKindSignatures,TypeAbstractions,NoTypeApplications,TypeFamilyDependencies #-}
import Data.Kind (Type, Constraint)
type C :: Type -> Constraint
class C a where
  type F @k a = r | r -> a
main :: IO ()
main = pure ()
