{-# LANGUAGE PolyKinds, TypeAbstractions, NoTypeApplications, RankNTypes, StandaloneKindSignatures, TypeFamilies #-}
import Data.Kind (Type, Constraint)
import qualified Data.Kind as Kind
class C a where
  type F @k a
type C :: Type -> Constraint
main :: IO ()
main = pure ()
