import Control.Monad.State
import Control.Monad.Reader
import Control.Monad.Writer
import Control.Monad.Except
import qualified Data.Map as M

type Eval a = ReaderT (M.Map String Int) (ExceptT String (StateT Int (Writer [String]))) a

data Expr = Lit Int | Var String | Add Expr Expr | Div Expr Expr

eval :: Expr -> Eval Int
eval (Lit n) = do
  modify (+ 1)
  return n
eval (Var x) = do
  env <- ask
  tell ["lookup " ++ x]
  case M.lookup x env of
    Just v -> return v
    Nothing -> throwError ("unbound " ++ x)
eval (Add a b) = (+) <$> eval a <*> eval b
eval (Div a b) = do
  x <- eval a
  y <- eval b
  when (y == 0) (throwError "divide by zero")
  return (x `div` y)

run :: Expr -> ((Either String Int, Int), [String])
run e = runWriter (runStateT (runExceptT (runReaderT (eval e) (M.fromList [("x", 10), ("y", 0)]))) 0)

counter :: State Int [Int]
counter = forM [1 .. 5] $ \i -> do
  n <- get
  put (n + i)
  gets (* 2)

main :: IO ()
main = do
  print (run (Add (Lit 1) (Var "x")))
  print (run (Div (Var "x") (Var "y")))
  print (run (Var "z"))
  print (runState counter 0)
  print (evalState (mapM (\x -> state (\s -> (x + s, s + 1))) [10, 20, 30]) 0)
  print (runReader (local (* 2) (asks (+ 1))) (5 :: Int))
  print (execWriter (mapM_ (\x -> when (odd x) (tell [x])) [1 .. 10 :: Int]))
  r <- execStateT (forM_ [1 .. 3 :: Int] (\i -> do { lift (print i); modify (* i) })) 1
  print r
