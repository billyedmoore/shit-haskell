{-# LANGUAGE StandaloneKindSignatures, NoTypeAbstractions #-}
import Data.Kind (Type)
type T :: Type -> Type
data T _ = T
main :: IO ()
main = pure ()
