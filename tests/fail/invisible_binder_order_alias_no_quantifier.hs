{-# LANGUAGE PolyKinds, TypeAbstractions, NoTypeApplications, RankNTypes, StandaloneKindSignatures #-}
import Data.Kind (Type, Constraint)
import qualified Data.Kind as Kind
type K = Type -> Type
type T :: K
data T @k a = T
main :: IO ()
main = pure ()
