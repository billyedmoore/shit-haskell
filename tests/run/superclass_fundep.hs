-- Functional dependencies of a superclass improve a wanted (megaparsec's
-- `class MonadParsec e s m => MonadParsecDbg e s m`, with `MonadParsec`'s `m -> e s`).
{-# LANGUAGE FunctionalDependencies, FlexibleInstances, FlexibleContexts #-}

class Monad m => MonadParsec e s m | m -> e s where
  token :: m s
  failWith :: e -> m a

class MonadParsec e s m => MonadParsecDbg e s m where
  dbg :: Show a => String -> m a -> m a

newtype P a = P { runP :: String -> Either Int (a, String) }

instance Functor P where fmap f (P p) = P (fmap (\(a, r) -> (f a, r)) . p)
instance Applicative P where
  pure a = P (\s -> Right (a, s))
  P f <*> P a = P (\s -> case f s of Left e -> Left e; Right (g, r) -> fmap (\(x, r') -> (g x, r')) (a r))
instance Monad P where P a >>= k = P (\s -> case a s of Left e -> Left e; Right (x, r) -> runP (k x) r)

instance MonadParsec Int Char P where
  token = P (\s -> case s of c : r -> Right (c, r); [] -> Left 0)
  failWith e = P (const (Left e))

instance MonadParsecDbg Int Char P where
  dbg lbl p = P (\s -> fmap (\(a, r) -> (a, r)) (runP p s)) <* pure (length lbl)

data ShowComment c a = ShowComment String (a, c)
instance (Show c, Show a) => Show (ShowComment c a) where
  show (ShowComment l (a, c)) = show a ++ " (" ++ l ++ ": " ++ show c ++ ")"

dbgWithComment :: (MonadParsecDbg e s m, Show a, Show c) => String -> String -> m (a, c) -> m (a, c)
dbgWithComment lbl str ma = (\(ShowComment _ v) -> v) <$> dbg str (ShowComment lbl <$> ma)

main :: IO ()
main = print (runP (dbgWithComment "x" "y" ((,) <$> token <*> token)) "abc")
