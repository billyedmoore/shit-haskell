-- GADTs whose parameters come only from a kind signature (hackage-security's Format, Formats).
{-# LANGUAGE GADTs, KindSignatures, StandaloneDeriving, TypeOperators #-}
import Data.Kind (Type)

data U
data G
data h :- t

data Format :: Type -> Type where
  FUn :: Format U
  FGz :: Format G

deriving instance Show (Format f)
deriving instance Eq (Format f)

data Formats :: Type -> Type -> Type where
  FsNone :: Formats () a
  FsUn :: a -> Formats (U :- ()) a
  FsUnGz :: a -> a -> Formats (U :- G :- ()) a

deriving instance Show a => Show (Formats fs a)

instance Functor (Formats fs) where
  fmap _ FsNone = FsNone
  fmap g (FsUn a) = FsUn (g a)
  fmap g (FsUnGz a b) = FsUnGz (g a) (g b)

class Named (f :: Type -> Type) where
  named :: f a -> String

instance Named Format where
  named FUn = "un"
  named FGz = "gz"

newtype Wrap :: Type -> Type where
  Wrap :: Int -> Wrap a

main :: IO ()
main = do
  print (FUn, FGz, FUn == FUn)
  print (fmap (+ 1) (FsUnGz 1 2 :: Formats (U :- G :- ()) Int), fmap not (FsUn True))
  putStrLn (named FUn ++ named FGz)
  case Wrap 3 :: Wrap Bool of Wrap n -> print n
